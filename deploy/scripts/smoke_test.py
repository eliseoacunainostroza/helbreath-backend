#!/usr/bin/env python3
"""
Helbreath backend smoke test runner.

Goals:
- Validate currently implemented backend functionality end-to-end.
- Be easy to extend as new features are added.
- Support quick core checks and full-stack checks (--full-stack).

How to extend:
- Add a new @register_test function in this file.
- Or add plugin files under deploy/scripts/smoke_tests.d/*.py (optional future extension).
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import signal
import socket
import subprocess
import sys
import time
import traceback
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple


ROOT = Path(__file__).resolve().parents[2]
LOG_DIR = ROOT / ".smoke" / "logs"

TestFunc = Callable[["Context"], None]
TESTS: List[Tuple[str, str, TestFunc]] = []


def register_test(test_id: str, description: str) -> Callable[[TestFunc], TestFunc]:
    def decorator(func: TestFunc) -> TestFunc:
        TESTS.append((test_id, description, func))
        return func

    return decorator


@dataclasses.dataclass
class Context:
    admin_base: str
    auth_base: str
    gateway_base: str
    gateway_tcp_host: str
    gateway_tcp_port: int
    world_base: str
    map_base: str
    chat_base: str
    jobs_base: str
    db_url: str
    admin_email: str
    admin_password: str
    verbose: bool
    with_db: bool
    full_stack: bool

    admin_token: Optional[str] = None
    fixture_account_id: Optional[str] = None
    fixture_account_username: Optional[str] = None
    fixture_account_password: Optional[str] = None
    fixture_character_id: Optional[str] = None
    fixture_character_name: Optional[str] = None

    started_processes: List[subprocess.Popen] = dataclasses.field(default_factory=list)
    service_logs: Dict[str, Path] = dataclasses.field(default_factory=dict)
    service_procs: Dict[str, subprocess.Popen] = dataclasses.field(default_factory=dict)
    launch_env: Optional[Dict[str, str]] = None


def load_env_file(path: Path) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        os.environ.setdefault(key, value)


def info(msg: str) -> None:
    print(f"[smoke] {msg}")


def ok(msg: str) -> None:
    print(f"[PASS] {msg}")


def warn(msg: str) -> None:
    print(f"[WARN] {msg}")


def fail(msg: str) -> None:
    print(f"[FAIL] {msg}")


def require_cmd(name: str) -> None:
    from shutil import which

    if which(name) is None:
        raise RuntimeError(f"required command not found: {name}")


def run_cmd(cmd: List[str], cwd: Optional[Path] = None, check: bool = True) -> subprocess.CompletedProcess:
    proc = subprocess.run(
        cmd,
        cwd=str(cwd or ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if check and proc.returncode != 0:
        raise RuntimeError(
            f"command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stdout}"
        )
    return proc


def http_json(
    method: str,
    url: str,
    body: Optional[Dict[str, Any]] = None,
    headers: Optional[Dict[str, str]] = None,
    timeout: float = 5.0,
) -> Tuple[int, Any, str]:
    payload_bytes = None
    req_headers = {"Accept": "application/json"}
    if headers:
        req_headers.update(headers)
    if body is not None:
        payload_bytes = json.dumps(body).encode("utf-8")
        req_headers.setdefault("Content-Type", "application/json")

    req = urllib.request.Request(url=url, method=method.upper(), data=payload_bytes, headers=req_headers)

    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            parsed = parse_maybe_json(raw)
            return resp.getcode(), parsed, raw
    except urllib.error.HTTPError as ex:
        raw = ex.read().decode("utf-8", errors="replace")
        parsed = parse_maybe_json(raw)
        return ex.code, parsed, raw


def parse_maybe_json(raw: str) -> Any:
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def parse_bind(raw: str, default_port: int) -> Tuple[str, int]:
    text = raw.strip()
    if ":" not in text:
        return text, default_port
    host, port_str = text.rsplit(":", 1)
    host = host.strip() or "127.0.0.1"
    port = int(port_str.strip())
    if host == "0.0.0.0":
        host = "127.0.0.1"
    return host, port


def can_bind_tcp(host: str, port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind((host, port))
            return True
        except OSError:
            return False


def choose_gateway_tcp_port(preferred_port: int) -> int:
    host = "127.0.0.1"
    if can_bind_tcp(host, preferred_port):
        return preferred_port
    for port in range(12848, 12949):
        if can_bind_tcp(host, port):
            return port
    raise RuntimeError("unable to find free TCP port for HB_GATEWAY_TCP_BIND")


def choose_bind_for_launch(
    env_key: str,
    preferred_port: int,
    range_start: int,
    range_end: int,
) -> Tuple[str, int]:
    host = "127.0.0.1"
    if can_bind_tcp(host, preferred_port):
        return host, preferred_port
    for port in range(range_start, range_end + 1):
        if can_bind_tcp(host, port):
            warn(f"{env_key} port {preferred_port} is busy; using {host}:{port} for smoke run")
            return host, port
    raise RuntimeError(f"unable to find free TCP port for {env_key}")


def encode_frame(opcode: int, payload: bytes) -> bytes:
    length = 2 + len(payload)
    return (
        int(length).to_bytes(2, "little")
        + int(opcode).to_bytes(2, "little")
        + payload
    )


def assert_status(status: int, expected: int, where: str) -> None:
    if status != expected:
        raise AssertionError(f"{where}: expected HTTP {expected}, got {status}")


def assert_true(value: bool, message: str) -> None:
    if not value:
        raise AssertionError(message)


def assert_prometheus_payload(body: Any, where: str) -> None:
    assert_true(isinstance(body, str), f"{where}: expected text/plain body")
    assert_true("hb_active_connections" in body, f"{where}: missing hb_active_connections metric")
    assert_true("hb_tick_duration_ms_last" in body, f"{where}: missing hb_tick_duration_ms_last metric")


def wait_until(
    predicate: Callable[[], bool],
    timeout_sec: float,
    interval_sec: float = 0.2,
) -> bool:
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        try:
            if predicate():
                return True
        except Exception:
            pass
        time.sleep(interval_sec)
    return False


def psql(ctx: Context, sql: str, quiet: bool = False) -> str:
    if not ctx.with_db:
        raise RuntimeError("psql called but --with-db is disabled")
    require_cmd("psql")

    cmd = ["psql", ctx.db_url, "-X", "-v", "ON_ERROR_STOP=1", "-At", "-c", sql]
    proc = run_cmd(cmd, check=True)
    out = proc.stdout.strip()
    if ctx.verbose and not quiet:
        info(f"psql> {sql}\n{out}")
    return out


def wait_http(url: str, timeout_sec: int = 45) -> bool:
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        try:
            status, _, _ = http_json("GET", url, timeout=2.0)
            if status < 500:
                return True
        except Exception:
            pass
        time.sleep(1)
    return False


def start_service(ctx: Context, crate: str) -> subprocess.Popen:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    log_path = LOG_DIR / f"{crate}.log"
    log_fp = open(log_path, "w", encoding="utf-8")
    proc = subprocess.Popen(
        ["cargo", "run", "-p", crate],
        cwd=str(ROOT),
        env=ctx.launch_env or os.environ.copy(),
        stdout=log_fp,
        stderr=subprocess.STDOUT,
        text=True,
    )
    ctx.started_processes.append(proc)
    ctx.service_logs[crate] = log_path
    ctx.service_procs[crate] = proc
    info(f"started {crate} (pid={proc.pid}) -> {log_path}")
    return proc


def stop_started_services(ctx: Context) -> None:
    for proc in reversed(ctx.started_processes):
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=8)
            except subprocess.TimeoutExpired:
                proc.kill()
    ctx.started_processes.clear()
    ctx.service_procs.clear()


def tail_text_file(path: Path, lines: int = 50) -> str:
    if not path.exists():
        return "(log file not found)"
    content = path.read_text(encoding="utf-8", errors="replace").splitlines()
    return "\n".join(content[-lines:])


def ensure_service_healthy(
    ctx: Context,
    crate: str,
    health_url: str,
    timeout_sec: int = 120,
) -> None:
    info(f"waiting health: {crate} -> {health_url}")
    deadline = time.time() + timeout_sec
    proc = ctx.service_procs.get(crate)
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            log_path = ctx.service_logs.get(crate)
            log_tail = tail_text_file(log_path) if log_path else "(no log path recorded)"
            fail(f"{crate} exited before becoming healthy (exit={proc.returncode})")
            fail(f"last log lines for {crate}:\n{log_tail}")
            raise AssertionError(f"{crate} exited before healthy")

        try:
            status, _, _ = http_json("GET", health_url, timeout=2.0)
            if status < 500:
                if proc is not None and proc.poll() is not None:
                    log_path = ctx.service_logs.get(crate)
                    log_tail = tail_text_file(log_path) if log_path else "(no log path recorded)"
                    fail(f"{crate} health responded but launched process is not running")
                    fail(f"last log lines for {crate}:\n{log_tail}")
                    raise AssertionError(f"{crate} not running (possible port conflict)")
                ok(f"{crate} healthy")
                return
        except Exception:
            pass
        time.sleep(1)

    log_path = ctx.service_logs.get(crate)
    log_tail = tail_text_file(log_path) if log_path else "(no log path recorded)"
    fail(f"{crate} did not become healthy within {timeout_sec}s")
    fail(f"last log lines for {crate}:\n{log_tail}")
    raise AssertionError(f"{crate} did not become healthy")


def wait_tcp_listener(host: str, port: int, timeout_sec: float = 8.0) -> bool:
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1.0):
                return True
        except Exception:
            time.sleep(0.2)
    return False


def setup_db(ctx: Context) -> None:
    require_cmd("docker")
    require_cmd("bash")

    info("docker compose up -d postgres redis")
    run_cmd(["docker", "compose", "up", "-d", "postgres", "redis"], check=True)

    info("running migrations")
    run_cmd(["bash", "deploy/scripts/migrate.sh"], check=True)


def seed_fixtures(ctx: Context) -> None:
    if not ctx.with_db:
        return

    account_id = str(uuid.uuid4())
    character_id = str(uuid.uuid4())
    inventory_id = str(uuid.uuid4())

    username = f"smk{int(time.time()) % 1000000}"
    char_name = f"smk{int(time.time()) % 100000}"[:16]
    item_code = f"smoke_item_{int(time.time())}"

    fixture_password = "smokepass123"
    # Smoke fixture marker consumed by auth service as explicit test account format.
    password_hash = f"plain:{fixture_password}"

    sql = f"""
    INSERT INTO accounts(id, username, email, password_hash, status)
    VALUES ('{account_id}', '{username}', '{username}@example.local', '{password_hash}', 'active');

    INSERT INTO characters(
      id, account_id, name, slot, class_code, map_id, pos_x, pos_y,
      level, exp, hp, mp, sp,
      str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat,
      is_deleted
    ) VALUES (
      '{character_id}', '{account_id}', '{char_name}', 0, 'warrior', 1, 100, 100,
      1, 0, 100, 50, 50,
      10, 10, 10, 10, 10, 10,
      false
    );

    INSERT INTO inventories(id, character_id, gold, version)
    VALUES ('{inventory_id}', '{character_id}', 1000, 0);

    INSERT INTO items(code, item_type, max_stack, attrs)
    VALUES ('{item_code}', 'misc', 99, '{{}}'::jsonb)
    ON CONFLICT (code) DO NOTHING;

    INSERT INTO inventory_items(inventory_id, item_id, slot, quantity, metadata)
    SELECT '{inventory_id}', id, 0, 3, '{{}}'::jsonb
    FROM items
    WHERE code = '{item_code}'
    LIMIT 1;
    """

    psql(ctx, sql, quiet=True)

    ctx.fixture_account_id = account_id
    ctx.fixture_account_username = username
    ctx.fixture_account_password = fixture_password
    ctx.fixture_character_id = character_id
    ctx.fixture_character_name = char_name

    info(f"seeded fixtures account={username} character={char_name}")


@register_test("health.admin", "Admin API health endpoint")
def test_health_admin(ctx: Context) -> None:
    status, body, _ = http_json("GET", f"{ctx.admin_base}/healthz")
    assert_status(status, 200, "admin health")
    assert_true(str(body).strip('"') == "ok", "admin health body expected 'ok'")


@register_test("health.auth", "Auth service health endpoint")
def test_health_auth(ctx: Context) -> None:
    status, _, _ = http_json("GET", f"{ctx.auth_base}/healthz")
    assert_status(status, 200, "auth health")


@register_test("health.gateway", "Gateway health endpoint")
def test_health_gateway(ctx: Context) -> None:
    status, _, _ = http_json("GET", f"{ctx.gateway_base}/healthz")
    assert_status(status, 200, "gateway health")


@register_test("health.gateway.ready", "Gateway readiness endpoint")
def test_health_gateway_ready(ctx: Context) -> None:
    status, body, _ = http_json("GET", f"{ctx.gateway_base}/readyz")
    assert_status(status, 200, "gateway readyz")
    assert_true(isinstance(body, dict), "gateway readyz payload must be object")
    if ctx.full_stack:
        assert_true(body.get("ok") is True, "gateway readyz should be ok in full-stack mode")


@register_test("health.world", "World service health endpoint")
def test_health_world(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping world health (run with --full-stack)")
        return
    status, _, _ = http_json("GET", f"{ctx.world_base}/healthz")
    assert_status(status, 200, "world health")


@register_test("health.map", "Map service health endpoint")
def test_health_map(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping map health (run with --full-stack)")
        return
    status, _, _ = http_json("GET", f"{ctx.map_base}/healthz")
    assert_status(status, 200, "map health")


@register_test("health.chat", "Chat service health endpoint")
def test_health_chat(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping chat health (run with --full-stack)")
        return
    status, _, _ = http_json("GET", f"{ctx.chat_base}/healthz")
    assert_status(status, 200, "chat health")


@register_test("health.jobs", "Jobs runner health endpoint")
def test_health_jobs(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping jobs health (run with --full-stack)")
        return
    status, _, _ = http_json("GET", f"{ctx.jobs_base}/healthz")
    assert_status(status, 200, "jobs health")


@register_test("metrics.prometheus", "Prometheus metrics endpoints")
def test_metrics_prometheus(ctx: Context) -> None:
    services = [
        ("admin", ctx.admin_base),
        ("auth", ctx.auth_base),
        ("gateway", ctx.gateway_base),
    ]
    if ctx.full_stack:
        services.extend(
            [
                ("world", ctx.world_base),
                ("map", ctx.map_base),
                ("chat", ctx.chat_base),
                ("jobs", ctx.jobs_base),
            ]
        )
    else:
        warn("prometheus metrics check limited to admin/auth/gateway (run with --full-stack for all services)")

    for name, base in services:
        status, body, _ = http_json("GET", f"{base}/metrics/prometheus")
        assert_status(status, 200, f"{name} metrics.prometheus")
        assert_prometheus_payload(body, f"{name} metrics.prometheus")


def ensure_admin_token(ctx: Context) -> str:
    if ctx.admin_token:
        return ctx.admin_token

    last_error = "sin detalle"
    for attempt in range(1, 6):
        try:
            status, body, raw = http_json(
                "POST",
                f"{ctx.admin_base}/api/v1/admin/login",
                body={"email": ctx.admin_email, "password": ctx.admin_password},
                timeout=3.0,
            )

            if status == 200 and isinstance(body, dict) and body.get("token"):
                ctx.admin_token = str(body["token"])
                return ctx.admin_token

            if status in (408, 429, 500, 502, 503, 504):
                last_error = f"status={status} raw={raw}"
            else:
                assert_status(status, 200, "admin login")
                assert_true(isinstance(body, dict), f"admin login non-json response: {raw}")
                token = body.get("token")
                assert_true(bool(token), "admin login did not return token")
                ctx.admin_token = str(token)
                return ctx.admin_token
        except Exception as exc:  # noqa: BLE001
            last_error = str(exc)

        if attempt < 5:
            time.sleep(0.35 * attempt)

    raise AssertionError(f"admin login failed after retries: {last_error}")


@register_test("admin.login", "Admin login with bootstrap credentials")
def test_admin_login(ctx: Context) -> None:
    ensure_admin_token(ctx)


@register_test("admin.dashboard", "Admin dashboard endpoint")
def test_admin_dashboard(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/dashboard",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "admin dashboard")
    assert_true(isinstance(body, dict), "dashboard must return JSON object")
    service_status = body.get("service_status")
    assert_true(isinstance(service_status, list), "dashboard service_status must be list")


@register_test("admin.services.status", "Admin services status endpoint")
def test_admin_services_status(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/services",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "admin services status")
    assert_true(isinstance(body, dict), "admin services status must return JSON object")

    rows = body.get("services")
    assert_true(isinstance(rows, list), "admin services status services must be list")
    expected = {"gateway", "auth", "world", "map", "chat", "jobs", "admin-api"}
    got = {str(item.get("service")) for item in rows if isinstance(item, dict)}
    assert_true(expected.issubset(got), f"admin services status missing services: expected={expected}, got={got}")

    for row in rows:
        assert_true(isinstance(row, dict), "service row must be object")
        assert_true(bool(row.get("unit")), f"service row missing unit: {row}")
        assert_true(bool(row.get("health")), f"service row missing health: {row}")
        assert_true(bool(row.get("unit_state")), f"service row missing unit_state: {row}")


@register_test("admin.services.logs", "Admin service logs endpoint")
def test_admin_services_logs(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/services/admin-api/logs?lines=60",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "admin services logs")
    assert_true(isinstance(body, dict), "admin services logs must return JSON object")
    assert_true(body.get("service") == "admin-api", "admin services logs service mismatch")
    assert_true(isinstance(body.get("log"), str), "admin services logs field 'log' must be string")


@register_test("admin.accounts.search", "Account search endpoint")
def test_accounts_search(ctx: Context) -> None:
    ensure_admin_token(ctx)
    query = ctx.fixture_account_username or ""
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/accounts?query={query}&limit=10",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "accounts search")
    assert_true(isinstance(body, list), "accounts search must return JSON array")


@register_test("admin.accounts.block-unblock", "Account block and unblock endpoints")
def test_accounts_block_unblock(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_account_id:
        warn("skipping account block/unblock (no DB fixtures)")
        return

    headers = {"Authorization": f"Bearer {ctx.admin_token}"}

    for action in ("block", "unblock"):
        status, body, _ = http_json(
            "POST",
            f"{ctx.admin_base}/api/v1/admin/accounts/{ctx.fixture_account_id}/{action}",
            headers=headers,
        )
        assert_status(status, 200, f"account {action}")
        assert_true(isinstance(body, dict) and body.get("ok") is True, f"account {action} failed")


@register_test("admin.accounts.reset-password", "Account reset-password workflow endpoint")
def test_accounts_reset_password(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_account_id:
        warn("skipping reset-password (no DB fixtures)")
        return

    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/accounts/{ctx.fixture_account_id}/reset-password",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "account reset-password")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "reset-password failed")
    assert_true(bool(body.get("workflow_ticket")), "reset-password missing workflow_ticket")


@register_test("admin.accounts.sanctions", "Account sanctions history endpoint")
def test_accounts_sanctions(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_account_id:
        warn("skipping sanctions list (no DB fixtures)")
        return

    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/accounts/{ctx.fixture_account_id}/sanctions",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "account sanctions")
    assert_true(isinstance(body, dict) and isinstance(body.get("rows"), list), "sanctions payload invalid")


@register_test("admin.characters.search", "Character search endpoint")
def test_characters_search(ctx: Context) -> None:
    ensure_admin_token(ctx)
    query = ctx.fixture_character_name or ""
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/characters?query={query}&limit=10",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "character search")
    assert_true(isinstance(body, list), "character search must return array")


@register_test("admin.characters.move", "Character move endpoint")
def test_characters_move(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_character_id:
        warn("skipping character move (no DB fixtures)")
        return

    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/characters/{ctx.fixture_character_id}/move",
        body={"map_id": 1, "x": 120, "y": 140},
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "character move")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "character move failed")


@register_test("admin.characters.disconnect", "Character disconnect endpoint")
def test_characters_disconnect(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_character_id:
        warn("skipping character disconnect (no DB fixtures)")
        return

    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/characters/{ctx.fixture_character_id}/disconnect",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "character disconnect")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "character disconnect failed")


@register_test("admin.characters.inventory", "Character inventory inspect endpoint")
def test_characters_inventory(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.fixture_character_id:
        warn("skipping character inventory (no DB fixtures)")
        return

    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/characters/{ctx.fixture_character_id}/inventory",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "character inventory")
    assert_true(isinstance(body, dict) and isinstance(body.get("items"), list), "character inventory invalid")


@register_test("admin.world.maps", "World maps endpoint")
def test_world_maps(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/world/maps",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "world maps")
    assert_true(isinstance(body, dict) and isinstance(body.get("maps"), list), "world maps invalid")


@register_test("admin.world.restart-map", "World map restart endpoint")
def test_world_restart_map(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.full_stack:
        warn("skipping world restart map (run with --full-stack)")
        return
    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/world/maps/1/restart",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "world restart map")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "world restart map failed")


@register_test("admin.world.broadcast", "World broadcast endpoint")
def test_world_broadcast(ctx: Context) -> None:
    ensure_admin_token(ctx)
    if not ctx.full_stack:
        warn("skipping world broadcast (run with --full-stack)")
        return
    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/world/broadcast",
        body={"message": "smoke broadcast"},
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "world broadcast")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "world broadcast failed")


@register_test("admin.world.toggle-event", "World event toggle endpoint")
def test_world_toggle_event(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/world/events/smoke_event/toggle",
        body={"enabled": True},
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "world toggle event")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "world toggle event failed")


@register_test("world.stats", "World stats endpoint")
def test_world_stats(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping world stats (run with --full-stack)")
        return
    status, body, _ = http_json("GET", f"{ctx.world_base}/v1/world/stats")
    assert_status(status, 200, "world stats")
    assert_true(isinstance(body, dict), "world stats payload must be object")
    assert_true("online_players" in body, "world stats missing online_players")


@register_test("world.route-command", "World route command endpoint")
def test_world_route_command(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping world route command (run with --full-stack)")
        return
    session_id = str(uuid.uuid4())
    status, body, _ = http_json(
        "POST",
        f"{ctx.world_base}/v1/world/maps/1/route",
        body={"session_id": session_id, "type": "heartbeat"},
    )
    assert_status(status, 200, "world route command")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "world route command failed")

    status, body, _ = http_json(
        "POST",
        f"{ctx.world_base}/v1/world/maps/1/route",
        body={"session_id": session_id, "type": "attack", "target_id": str(uuid.uuid4())},
    )
    assert_status(status, 200, "world route attack")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "world route attack failed")


def run_gateway_tcp_route_flow(ctx: Context, client_version: str, label: str) -> None:
    assert_true(bool(ctx.fixture_account_username), "fixture account username missing")
    assert_true(bool(ctx.fixture_account_password), "fixture account password missing")
    assert_true(bool(ctx.fixture_character_id), "fixture character id missing")

    status, before_body, _ = http_json("GET", f"{ctx.world_base}/v1/world/stats")
    assert_status(status, 200, f"world stats before tcp flow ({label})")
    assert_true(isinstance(before_body, dict), "world stats before must be object")
    before_online = int(before_body.get("online_players", 0))

    login_payload = (
        ctx.fixture_account_username.encode("utf-8")
        + b"\0"
        + ctx.fixture_account_password.encode("utf-8")
        + b"\0"
        + client_version.encode("utf-8")
        + b"\0"
    )
    frames = [
        encode_frame(0x0001, login_payload),  # login
        encode_frame(0x0002, b""),  # character list
        encode_frame(0x0004, uuid.UUID(ctx.fixture_character_id).bytes),  # character select
        encode_frame(0x0005, b""),  # enter world
        encode_frame(
            0x0100,
            (130).to_bytes(4, "little", signed=True)
            + (145).to_bytes(4, "little", signed=True)
            + b"\x01",
        ),  # move
    ]

    last_during_online = before_online

    def fetch_online_players() -> int:
        status, payload, _ = http_json("GET", f"{ctx.world_base}/v1/world/stats")
        assert_status(status, 200, "world stats poll")
        assert_true(isinstance(payload, dict), "world stats poll must be object")
        return int(payload.get("online_players", 0))

    with socket.create_connection((ctx.gateway_tcp_host, ctx.gateway_tcp_port), timeout=3.0) as s:
        for frame in frames:
            s.sendall(frame)
            time.sleep(0.05)

        became_online = wait_until(
            lambda: fetch_online_players() >= before_online + 1,
            timeout_sec=6.0,
            interval_sec=0.25,
        )
        during_online = fetch_online_players()
        last_during_online = during_online
        assert_true(
            became_online and during_online >= before_online + 1,
            f"expected online_players to increase ({label}) (before={before_online}, during={during_online})",
        )

        s.sendall(encode_frame(0x02FF, b""))  # logout
        wait_until(
            lambda: fetch_online_players() <= max(before_online, last_during_online - 1),
            timeout_sec=6.0,
            interval_sec=0.25,
        )

    after_online = fetch_online_players()
    assert_true(
        after_online <= last_during_online,
        f"expected online_players to not increase after logout ({label}) (during={last_during_online}, after={after_online})",
    )


@register_test("gateway.tcp.route-flow", "Gateway TCP flow routes enter-world to world-service")
def test_gateway_tcp_route_flow(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping gateway tcp route flow (run with --full-stack)")
        return
    if not ctx.with_db:
        warn("skipping gateway tcp route flow (requires --with-db fixture)")
        return
    run_gateway_tcp_route_flow(ctx, "4.96", "legacy")


@register_test(
    "gateway.tcp.route-flow.modern",
    "Gateway TCP modern flow routes enter-world to world-service",
)
def test_gateway_tcp_route_flow_modern(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping gateway tcp route flow modern (run with --full-stack)")
        return
    if not ctx.with_db:
        warn("skipping gateway tcp route flow modern (requires --with-db fixture)")
        return
    run_gateway_tcp_route_flow(ctx, "modern-v400", "modern")


@register_test("map.ping", "Map ping endpoint")
def test_map_ping(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping map ping (run with --full-stack)")
        return
    status, body, _ = http_json("GET", f"{ctx.map_base}/v1/maps/1/ping")
    assert_status(status, 200, "map ping")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "map ping failed")


@register_test("map.enter-move", "Map enter-world and move command endpoints")
def test_map_enter_move(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping map enter/move (run with --full-stack)")
        return
    status, body, _ = http_json("POST", f"{ctx.map_base}/v1/maps/1/enter-world")
    assert_status(status, 200, "map enter-world")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "map enter-world failed")

    status, body, _ = http_json(
        "POST",
        f"{ctx.map_base}/v1/maps/1/move",
        body={"x": 130, "y": 145, "run": True},
    )
    assert_status(status, 200, "map move")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "map move failed")


@register_test("admin.moderation", "Moderation endpoints mute/jail/ban")
def test_moderation(ctx: Context) -> None:
    ensure_admin_token(ctx)
    headers = {"Authorization": f"Bearer {ctx.admin_token}"}
    payload = {
        "scope": "account",
        "account_id": ctx.fixture_account_id,
        "reason": "smoke moderation",
        "minutes": 5,
    }

    for endpoint in ("mute", "jail", "ban"):
        status, body, _ = http_json(
            "POST",
            f"{ctx.admin_base}/api/v1/admin/moderation/{endpoint}",
            body=payload,
            headers=headers,
        )
        assert_status(status, 200, f"moderation {endpoint}")
        assert_true(isinstance(body, dict) and body.get("ok") is True, f"moderation {endpoint} failed")


@register_test("admin.audit", "Audit endpoint includes actions")
def test_audit(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/audit?limit=50",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "audit list")
    assert_true(isinstance(body, dict) and isinstance(body.get("rows"), list), "audit payload invalid")


@register_test("auth.login.negative", "Auth endpoint rejects invalid credentials")
def test_auth_negative_login(ctx: Context) -> None:
    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/login",
        body={
            "session_id": str(uuid.uuid4()),
            "username": "does_not_exist",
            "password": "badpass",
            "remote_ip": "127.0.0.1",
        },
    )
    assert_status(status, 200, "auth login negative")
    assert_true(isinstance(body, dict) and body.get("accepted") is False, "invalid auth should be rejected")


@register_test("auth.login.positive", "Auth endpoint accepts fixture credentials")
def test_auth_positive_login(ctx: Context) -> None:
    if not ctx.with_db:
        warn("skipping auth positive login (requires --with-db fixture)")
        return
    assert_true(bool(ctx.fixture_account_username), "fixture account username missing")
    assert_true(bool(ctx.fixture_account_password), "fixture account password missing")

    session_id = str(uuid.uuid4())
    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/login",
        body={
            "session_id": session_id,
            "username": ctx.fixture_account_username,
            "password": ctx.fixture_account_password,
            "remote_ip": "127.0.0.1",
        },
    )
    assert_status(status, 200, "auth login positive")
    assert_true(isinstance(body, dict), "auth login positive payload must be object")
    assert_true(
        body.get("accepted") is True,
        f"fixture auth login should be accepted, got body={body}",
    )

    # cleanup session to keep DB state clean across runs
    http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/logout",
        body={"session_id": session_id},
    )


@register_test("auth.character.lifecycle", "Auth character list/create/select/delete lifecycle")
def test_auth_character_lifecycle(ctx: Context) -> None:
    if not ctx.with_db:
        warn("skipping auth character lifecycle (requires --with-db fixture)")
        return
    assert_true(bool(ctx.fixture_account_username), "fixture account username missing")
    assert_true(bool(ctx.fixture_account_password), "fixture account password missing")

    session_id = str(uuid.uuid4())
    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/login",
        body={
            "session_id": session_id,
            "username": ctx.fixture_account_username,
            "password": ctx.fixture_account_password,
            "remote_ip": "127.0.0.1",
        },
    )
    assert_status(status, 200, "auth lifecycle login")
    assert_true(
        isinstance(body, dict) and body.get("accepted") is True,
        f"auth lifecycle login failed, body={body}",
    )

    status, body, _ = http_json(
        "GET",
        f"{ctx.auth_base}/v1/auth/characters?session_id={session_id}",
    )
    assert_status(status, 200, "auth lifecycle list before")
    assert_true(isinstance(body, dict), "auth lifecycle list before payload invalid")
    before = body.get("characters", [])
    assert_true(isinstance(before, list), "auth lifecycle before characters invalid")

    temp_name = f"smk{int(time.time()) % 1000000}"[:16]
    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/characters/create",
        body={
            "session_id": session_id,
            "name": temp_name,
            "class": "warrior",
            "gender": 0,
            "skin_color": 0,
            "hair_style": 0,
            "hair_color": 0,
            "underwear_color": 0,
            "stats": [10, 10, 10, 10, 10, 10],
        },
    )
    assert_status(status, 200, "auth lifecycle create")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "auth lifecycle create failed")
    char = body.get("character") or {}
    char_id = char.get("character_id")
    assert_true(bool(char_id), "auth lifecycle create missing character_id")

    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/characters/select",
        body={"session_id": session_id, "character_id": char_id},
    )
    assert_status(status, 200, "auth lifecycle select")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "auth lifecycle select failed")
    assert_true(body.get("map_id") is not None, "auth lifecycle select missing map_id")

    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/characters/delete",
        body={"session_id": session_id, "character_id": char_id},
    )
    assert_status(status, 200, "auth lifecycle delete")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "auth lifecycle delete failed")

    http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/logout",
        body={"session_id": session_id},
    )


@register_test("auth.logout", "Auth logout endpoint closes session id")
def test_auth_logout(ctx: Context) -> None:
    status, body, _ = http_json(
        "POST",
        f"{ctx.auth_base}/v1/auth/logout",
        body={"session_id": str(uuid.uuid4())},
    )
    assert_status(status, 200, "auth logout")
    assert_true(isinstance(body, dict) and "ok" in body, "auth logout payload invalid")


@register_test("chat.map-outbound", "Chat map message and outbound inspection endpoints")
def test_chat_map_outbound(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping chat tests (run with --full-stack)")
        return

    sender = str(uuid.uuid4())
    status, body, _ = http_json(
        "POST",
        f"{ctx.chat_base}/v1/chat/map",
        body={"from_character_id": sender, "map_id": 1, "message": "smoke says hi"},
    )
    assert_status(status, 200, "chat map send")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "chat map send failed")

    time.sleep(0.2)
    status, body, _ = http_json("GET", f"{ctx.chat_base}/v1/chat/outbound?limit=20")
    assert_status(status, 200, "chat outbound")
    assert_true(isinstance(body, dict), "chat outbound payload invalid")
    rows = body.get("rows")
    assert_true(isinstance(rows, list), "chat outbound rows should be list")
    assert_true(
        any(isinstance(row, dict) and row.get("message") == "smoke says hi" for row in rows),
        "chat outbound did not include sent message",
    )


@register_test("jobs.status", "Jobs runner status endpoint")
def test_jobs_status(ctx: Context) -> None:
    if not ctx.full_stack:
        warn("skipping jobs status (run with --full-stack)")
        return
    status, body, _ = http_json("GET", f"{ctx.jobs_base}/v1/jobs/status")
    assert_status(status, 200, "jobs status")
    assert_true(isinstance(body, dict), "jobs status payload must be object")
    assert_true("ticks_total" in body, "jobs status missing ticks_total")


@register_test("admin.logout", "Admin logout endpoint")
def test_admin_logout(ctx: Context) -> None:
    ensure_admin_token(ctx)
    status, body, _ = http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/logout",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_status(status, 200, "admin logout")
    assert_true(isinstance(body, dict) and body.get("ok") is True, "admin logout failed")


@register_test("rbac.after-logout", "Protected endpoint denied after logout")
def test_rbac_after_logout(ctx: Context) -> None:
    ensure_admin_token(ctx)
    http_json(
        "POST",
        f"{ctx.admin_base}/api/v1/admin/logout",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    status, _, _ = http_json(
        "GET",
        f"{ctx.admin_base}/api/v1/admin/dashboard",
        headers={"Authorization": f"Bearer {ctx.admin_token}"},
    )
    assert_true(status in (401, 403), f"expected unauthorized/forbidden after logout, got {status}")


def run_tests(ctx: Context, only_prefix: Optional[str]) -> int:
    selected = [item for item in TESTS if (only_prefix is None or item[0].startswith(only_prefix))]
    if not selected:
        fail("no tests selected")
        return 1

    passed = 0
    failed = 0

    info(f"running {len(selected)} tests")
    for test_id, description, func in selected:
        try:
            func(ctx)
            ok(f"{test_id} - {description}")
            passed += 1
        except Exception as exc:
            fail(f"{test_id} - {description}: {exc}")
            if ctx.verbose:
                traceback.print_exc()
            failed += 1

    print("\n=== Smoke Summary ===")
    print(f"passed: {passed}")
    print(f"failed: {failed}")
    print(f"total : {passed + failed}")

    return 0 if failed == 0 else 2


def main() -> int:
    parser = argparse.ArgumentParser(description="Helbreath backend smoke test runner")
    parser.add_argument("--setup", action="store_true", help="run docker compose + migrations before tests")
    parser.add_argument("--launch", action="store_true", help="launch gateway/auth/admin services with cargo run")
    parser.add_argument("--full-stack", action="store_true", help="include world/map/chat/jobs services and tests")
    parser.add_argument("--keep-running", action="store_true", help="do not stop launched services on exit")
    parser.add_argument("--with-db", action="store_true", help="seed DB fixtures and validate DB-related endpoints")
    parser.add_argument("--cargo-check", action="store_true", help="run cargo check --workspace before tests")
    parser.add_argument("--only", default=None, help="run only tests with ID prefix (example: admin.world)")
    parser.add_argument("--verbose", action="store_true", help="verbose output")

    args = parser.parse_args()

    load_env_file(ROOT / ".env")
    load_env_file(ROOT / ".env.example")

    admin_host, admin_port = parse_bind(
        os.environ.get("HB_ADMIN_BIND", "127.0.0.1:8080"),
        8080,
    )
    auth_host, auth_port = parse_bind(
        os.environ.get("HB_AUTH_BIND", "127.0.0.1:7101"),
        7101,
    )
    gateway_http_host, gateway_http_port = parse_bind(
        os.environ.get("HB_GATEWAY_HTTP_BIND", "127.0.0.1:7080"),
        7080,
    )
    world_host, world_port = parse_bind(
        os.environ.get("HB_WORLD_BIND", "127.0.0.1:7201"),
        7201,
    )
    map_host, map_port = parse_bind(
        os.environ.get("HB_MAP_BIND", "127.0.0.1:7301"),
        7301,
    )
    chat_host, chat_port = parse_bind(
        os.environ.get("HB_CHAT_BIND", "127.0.0.1:7401"),
        7401,
    )
    jobs_host, jobs_port = parse_bind(
        os.environ.get("HB_JOBS_BIND", "127.0.0.1:7501"),
        7501,
    )
    gateway_tcp_host, gateway_tcp_port = parse_bind(
        os.environ.get("HB_GATEWAY_TCP_BIND", "0.0.0.0:2848"),
        2848,
    )
    launch_env = os.environ.copy()
    if args.launch:
        admin_host, admin_port = choose_bind_for_launch("HB_ADMIN_BIND", admin_port, 18080, 18179)
        auth_host, auth_port = choose_bind_for_launch("HB_AUTH_BIND", auth_port, 17101, 17200)
        gateway_http_host, gateway_http_port = choose_bind_for_launch(
            "HB_GATEWAY_HTTP_BIND", gateway_http_port, 17080, 17179
        )
        world_host, world_port = choose_bind_for_launch("HB_WORLD_BIND", world_port, 17201, 17300)
        map_host, map_port = choose_bind_for_launch("HB_MAP_BIND", map_port, 17301, 17400)
        chat_host, chat_port = choose_bind_for_launch("HB_CHAT_BIND", chat_port, 17401, 17500)
        jobs_host, jobs_port = choose_bind_for_launch("HB_JOBS_BIND", jobs_port, 17501, 17600)

        chosen_tcp_port = choose_gateway_tcp_port(gateway_tcp_port)
        if chosen_tcp_port != gateway_tcp_port:
            warn(
                f"HB_GATEWAY_TCP_BIND port {gateway_tcp_port} is busy; using 127.0.0.1:{chosen_tcp_port} for smoke run"
            )
        gateway_tcp_host = "127.0.0.1"
        gateway_tcp_port = chosen_tcp_port
        launch_env["HB_ADMIN_BIND"] = f"{admin_host}:{admin_port}"
        launch_env["HB_AUTH_BIND"] = f"{auth_host}:{auth_port}"
        launch_env["HB_GATEWAY_HTTP_BIND"] = f"{gateway_http_host}:{gateway_http_port}"
        launch_env["HB_WORLD_BIND"] = f"{world_host}:{world_port}"
        launch_env["HB_MAP_BIND"] = f"{map_host}:{map_port}"
        launch_env["HB_CHAT_BIND"] = f"{chat_host}:{chat_port}"
        launch_env["HB_JOBS_BIND"] = f"{jobs_host}:{jobs_port}"
        launch_env["HB_GATEWAY_TCP_BIND"] = f"{gateway_tcp_host}:{gateway_tcp_port}"

    ctx = Context(
        admin_base=f"http://{admin_host}:{admin_port}",
        auth_base=f"http://{auth_host}:{auth_port}",
        gateway_base=f"http://{gateway_http_host}:{gateway_http_port}",
        gateway_tcp_host=gateway_tcp_host,
        gateway_tcp_port=gateway_tcp_port,
        world_base=f"http://{world_host}:{world_port}",
        map_base=f"http://{map_host}:{map_port}",
        chat_base=f"http://{chat_host}:{chat_port}",
        jobs_base=f"http://{jobs_host}:{jobs_port}",
        db_url=os.environ.get("HB_DATABASE_URL", "postgres://hb:hbpass@127.0.0.1:5432/helbreath"),
        admin_email=os.environ.get("HB_BOOTSTRAP_ADMIN_EMAIL", "admin@localhost"),
        admin_password=os.environ.get("HB_BOOTSTRAP_ADMIN_PASSWORD", "change_me_now"),
        verbose=args.verbose,
        with_db=args.with_db,
        full_stack=args.full_stack,
        launch_env=launch_env,
    )

    try:
        if args.setup:
            setup_db(ctx)

        if args.cargo_check:
            require_cmd("cargo")
            info("running cargo check --workspace")
            run_cmd(["cargo", "check", "--workspace"], check=True)

        if args.with_db:
            seed_fixtures(ctx)

        if args.launch:
            require_cmd("cargo")
            start_service(ctx, "hb-admin-api")
            start_service(ctx, "hb-auth-service")
            start_service(ctx, "hb-gateway")
            if args.full_stack:
                start_service(ctx, "hb-world-service")
                start_service(ctx, "hb-map-server")
                start_service(ctx, "hb-chat-service")
                start_service(ctx, "hb-jobs-runner")

            ensure_service_healthy(ctx, "hb-admin-api", f"{ctx.admin_base}/healthz", 120)
            ensure_service_healthy(ctx, "hb-auth-service", f"{ctx.auth_base}/healthz", 120)
            ensure_service_healthy(ctx, "hb-gateway", f"{ctx.gateway_base}/healthz", 120)
            if not wait_tcp_listener(ctx.gateway_tcp_host, ctx.gateway_tcp_port, timeout_sec=10.0):
                gateway_log_path = ctx.service_logs.get("hb-gateway")
                gateway_log = (
                    tail_text_file(gateway_log_path)
                    if gateway_log_path is not None
                    else "(gateway log path unavailable)"
                )
                raise AssertionError(
                    f"gateway TCP listener not reachable on {ctx.gateway_tcp_host}:{ctx.gateway_tcp_port}\n{gateway_log}"
                )
            if args.full_stack:
                ensure_service_healthy(ctx, "hb-world-service", f"{ctx.world_base}/healthz", 120)
                ensure_service_healthy(ctx, "hb-map-server", f"{ctx.map_base}/healthz", 120)
                ensure_service_healthy(ctx, "hb-chat-service", f"{ctx.chat_base}/healthz", 120)
                ensure_service_healthy(ctx, "hb-jobs-runner", f"{ctx.jobs_base}/healthz", 120)

        exit_code = run_tests(ctx, args.only)

    finally:
        if args.launch and not args.keep_running:
            stop_started_services(ctx)

    return exit_code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\n[smoke] interrupted")
        raise SystemExit(130)
