#!/usr/bin/env python3
"""
Run smoke tests repeatedly to detect flaky behavior.
"""

from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import List

ROOT = Path(__file__).resolve().parents[2]


@dataclass
class IterationResult:
    iteration: int
    return_code: int
    duration_seconds: float
    command: List[str]

    @property
    def ok(self) -> bool:
        return self.return_code == 0


def build_smoke_command(args: argparse.Namespace, iteration: int) -> List[str]:
    cmd = [
        sys.executable,
        str(ROOT / "deploy/scripts/smoke_test.py"),
        "--launch",
    ]
    if args.full_stack:
        cmd.append("--full-stack")
    if args.with_db:
        cmd.append("--with-db")
    if args.verbose:
        cmd.append("--verbose")
    if args.only:
        for test_id in args.only:
            cmd.extend(["--only", test_id])

    setup_mode = "none"
    if args.setup_each:
        setup_mode = "each"
    elif args.setup_first:
        setup_mode = "first"

    if setup_mode == "each" or (setup_mode == "first" and iteration == 1):
        cmd.append("--setup")
    return cmd


def run_once(args: argparse.Namespace, iteration: int) -> IterationResult:
    cmd = build_smoke_command(args, iteration)
    printable = shlex.join(cmd)
    print(f"[soak] iteracion {iteration}/{args.iterations}: {printable}")
    start = time.monotonic()
    completed = subprocess.run(cmd, cwd=ROOT, check=False)
    duration = time.monotonic() - start
    status = "ok" if completed.returncode == 0 else "fail"
    print(
        f"[soak] iteracion {iteration}/{args.iterations} {status} "
        f"(rc={completed.returncode}, {duration:.2f}s)"
    )
    return IterationResult(
        iteration=iteration,
        return_code=completed.returncode,
        duration_seconds=duration,
        command=cmd,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run repeated smoke tests (soak mode)")
    parser.add_argument(
        "--iterations",
        type=int,
        default=3,
        help="how many smoke runs to execute (default: 3)",
    )
    parser.add_argument(
        "--delay-seconds",
        type=float,
        default=3.0,
        help="sleep between iterations (default: 3)",
    )
    parser.add_argument(
        "--full-stack",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="run smoke in full-stack mode (default: true)",
    )
    parser.add_argument(
        "--with-db",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="run smoke with DB fixtures (default: true)",
    )
    parser.add_argument(
        "--setup-first",
        action="store_true",
        help="run smoke --setup only in the first iteration",
    )
    parser.add_argument(
        "--setup-each",
        action="store_true",
        help="run smoke --setup in every iteration",
    )
    parser.add_argument(
        "--only",
        action="append",
        default=[],
        help="run smoke tests filtered by id prefix (repeatable)",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="forward verbose mode to smoke_test.py",
    )
    parser.add_argument(
        "--stop-on-fail",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="stop after first failed iteration (default: true)",
    )
    parser.add_argument(
        "--json-output",
        default="",
        help="optional output file with machine-readable summary",
    )
    return parser.parse_args()


def write_json_summary(
    path: Path,
    args: argparse.Namespace,
    started_at: datetime,
    finished_at: datetime,
    results: List[IterationResult],
) -> None:
    payload = {
        "started_at": started_at.isoformat(),
        "finished_at": finished_at.isoformat(),
        "iterations_requested": args.iterations,
        "iterations_executed": len(results),
        "passed": sum(1 for item in results if item.ok),
        "failed": sum(1 for item in results if not item.ok),
        "config": {
            "full_stack": args.full_stack,
            "with_db": args.with_db,
            "setup_first": args.setup_first,
            "setup_each": args.setup_each,
            "only": args.only,
            "verbose": args.verbose,
            "stop_on_fail": args.stop_on_fail,
            "delay_seconds": args.delay_seconds,
        },
        "runs": [asdict(item) for item in results],
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"[soak] resumen json -> {path}")


def main() -> int:
    args = parse_args()
    if args.iterations < 1:
        raise SystemExit("--iterations debe ser >= 1")
    if args.delay_seconds < 0:
        raise SystemExit("--delay-seconds no puede ser negativo")
    if args.setup_first and args.setup_each:
        raise SystemExit("use solo uno: --setup-first o --setup-each")

    started_at = datetime.now(timezone.utc)
    results: List[IterationResult] = []

    for iteration in range(1, args.iterations + 1):
        result = run_once(args, iteration)
        results.append(result)
        if not result.ok and args.stop_on_fail:
            print("[soak] abortando por fallo (stop-on-fail=true)")
            break
        if iteration < args.iterations:
            time.sleep(args.delay_seconds)

    finished_at = datetime.now(timezone.utc)
    passed = sum(1 for item in results if item.ok)
    failed = sum(1 for item in results if not item.ok)
    total_duration = sum(item.duration_seconds for item in results)
    print(
        f"[soak] resumen: passed={passed} failed={failed} "
        f"executed={len(results)} duration={total_duration:.2f}s"
    )

    if args.json_output.strip():
        write_json_summary(
            Path(args.json_output),
            args,
            started_at,
            finished_at,
            results,
        )

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
