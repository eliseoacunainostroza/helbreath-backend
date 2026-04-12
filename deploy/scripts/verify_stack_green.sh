#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if [[ -f ".env" ]]; then
  set -a
  # shellcheck source=/dev/null
  source ".env"
  set +a
fi

if [[ -z "${HB_DATABASE_URL:-}" ]]; then
  echo "[verify-stack] HB_DATABASE_URL is required (env or .env)"
  exit 1
fi

if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="/var/tmp/helbreath-cargo-target"
fi
mkdir -p "${CARGO_TARGET_DIR}"

BIN_DIR="${BIN_DIR:-${ROOT_DIR}/bin}"
mkdir -p "${BIN_DIR}"

RUN_TESTS="${RUN_TESTS:-1}"
RUN_SMOKE="${RUN_SMOKE:-1}"
RUN_SMOKE_LAUNCH="${RUN_SMOKE_LAUNCH:-0}"
RUN_RELEASE_BUILD="${RUN_RELEASE_BUILD:-1}"
INSTALL_BINARIES="${INSTALL_BINARIES:-1}"
RESTART_SERVICES="${RESTART_SERVICES:-1}"

SERVICES=(
  hb-gateway
  hb-auth
  hb-world
  hb-map
  hb-chat
  hb-jobs
  hb-admin-api
)

PACKAGES=(
  hb-gateway
  hb-auth-service
  hb-world-service
  hb-map-server
  hb-chat-service
  hb-jobs-runner
  hb-admin-api
)

RELEASE_BINS=(
  hb-gateway
  hb-auth-service
  hb-world-service
  hb-map-server
  hb-chat-service
  hb-jobs-runner
  hb-admin-api
)

require_cmd() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "[verify-stack] command not found: $name"
    exit 1
  fi
}

bind_to_health_url() {
  local bind="$1"
  local default_port="$2"
  local host="127.0.0.1"
  local port="${default_port}"
  if [[ "$bind" == *:* ]]; then
    host="${bind%:*}"
    port="${bind##*:}"
  fi
  if [[ -z "$host" || "$host" == "0.0.0.0" ]]; then
    host="127.0.0.1"
  fi
  echo "http://${host}:${port}/healthz"
}

wait_for_health() {
  local service="$1"
  local url="$2"
  local timeout_sec="${3:-60}"
  local interval_sec=2
  local elapsed=0

  while (( elapsed < timeout_sec )); do
    if curl -fsS "${url}" >/dev/null 2>&1; then
      return 0
    fi
    sleep "${interval_sec}"
    elapsed=$((elapsed + interval_sec))
  done

  echo "[verify-stack] health probe timed out for ${service}: ${url}"
  sudo systemctl status "${service}" --no-pager -l || true
  sudo journalctl -u "${service}" -n 80 --no-pager || true
  return 1
}

require_cmd cargo
require_cmd python3
require_cmd psql
require_cmd curl

echo "[verify-stack] root=${ROOT_DIR}"
echo "[verify-stack] cargo_target_dir=${CARGO_TARGET_DIR}"
echo "[verify-stack] bin_dir=${BIN_DIR}"

echo "[verify-stack] step 1/6: migrate database"
bash deploy/scripts/migrate.sh

echo "[verify-stack] step 2/6: verify db schema/data"
psql "${HB_DATABASE_URL}" -v ON_ERROR_STOP=1 -c "SELECT current_database() AS db, current_user AS role;"
psql "${HB_DATABASE_URL}" -v ON_ERROR_STOP=1 -c "SELECT to_regclass('public.maps') AS maps_table, to_regclass('public.map_instances') AS map_instances_table, to_regclass('public.sanctions') AS sanctions_table;"
psql "${HB_DATABASE_URL}" -v ON_ERROR_STOP=1 -c "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='sanctions' AND column_name='status') AS sanctions_status_column;"
psql "${HB_DATABASE_URL}" -v ON_ERROR_STOP=1 -c "SELECT COUNT(*) AS maps_total FROM maps;"

if [[ "${RUN_TESTS}" == "1" ]]; then
  echo "[verify-stack] step 3/6: cargo test --workspace"
  cargo test --workspace
else
  echo "[verify-stack] step 3/6: skipped tests (RUN_TESTS=${RUN_TESTS})"
fi

if [[ "${RUN_RELEASE_BUILD}" == "1" ]]; then
  echo "[verify-stack] step 4/6: release build"
  cargo build --release \
    -p "${PACKAGES[0]}" \
    -p "${PACKAGES[1]}" \
    -p "${PACKAGES[2]}" \
    -p "${PACKAGES[3]}" \
    -p "${PACKAGES[4]}" \
    -p "${PACKAGES[5]}" \
    -p "${PACKAGES[6]}"
else
  echo "[verify-stack] step 4/6: skipped release build (RUN_RELEASE_BUILD=${RUN_RELEASE_BUILD})"
fi

if [[ "${INSTALL_BINARIES}" == "1" ]]; then
  if [[ "${RESTART_SERVICES}" == "1" ]]; then
    echo "[verify-stack] pre-stop services before install (avoid Text file busy)"
    for svc in "${SERVICES[@]}"; do
      sudo systemctl stop "${svc}" || true
    done
  fi

  echo "[verify-stack] installing release binaries into ${BIN_DIR}"
  for bin in "${RELEASE_BINS[@]}"; do
    src="${CARGO_TARGET_DIR}/release/${bin}"
    if [[ ! -f "${src}" ]]; then
      echo "[verify-stack] missing release binary: ${src}"
      exit 1
    fi
    sudo install -m 0755 "${src}" "${BIN_DIR}/${bin}"
  done
fi

if [[ "${RESTART_SERVICES}" == "1" ]]; then
  echo "[verify-stack] step 5/6: clean restart services"
  for svc in "${SERVICES[@]}"; do
    sudo systemctl stop "${svc}" || true
  done
  for svc in "${SERVICES[@]}"; do
    sudo systemctl start "${svc}"
  done
  for svc in "${SERVICES[@]}"; do
    sudo systemctl is-active --quiet "${svc}" || {
      echo "[verify-stack] service not active: ${svc}"
      sudo systemctl status "${svc}" --no-pager -l || true
      exit 1
    }
  done

  echo "[verify-stack] probing /healthz for every service"
  gateway_url="$(bind_to_health_url "${HB_GATEWAY_HTTP_BIND:-127.0.0.1:7080}" "7080")"
  auth_url="$(bind_to_health_url "${HB_AUTH_BIND:-127.0.0.1:7101}" "7101")"
  world_url="$(bind_to_health_url "${HB_WORLD_BIND:-127.0.0.1:7201}" "7201")"
  map_url="$(bind_to_health_url "${HB_MAP_BIND:-127.0.0.1:7301}" "7301")"
  chat_url="$(bind_to_health_url "${HB_CHAT_BIND:-127.0.0.1:7401}" "7401")"
  jobs_url="$(bind_to_health_url "${HB_JOBS_BIND:-127.0.0.1:7501}" "7501")"
  admin_url="$(bind_to_health_url "${HB_ADMIN_BIND:-127.0.0.1:8080}" "8080")"

  health_pairs=(
    "hb-gateway|${gateway_url}"
    "hb-auth|${auth_url}"
    "hb-world|${world_url}"
    "hb-map|${map_url}"
    "hb-chat|${chat_url}"
    "hb-jobs|${jobs_url}"
    "hb-admin-api|${admin_url}"
  )

  for pair in "${health_pairs[@]}"; do
    service="${pair%%|*}"
    url="${pair#*|}"
    echo "[verify-stack] wait health ${service}: ${url}"
    wait_for_health "${service}" "${url}" "${HEALTH_WAIT_TIMEOUT_SEC:-60}"
  done
fi

if [[ "${RUN_SMOKE}" == "1" ]]; then
  if [[ "${RUN_SMOKE_LAUNCH}" == "1" ]]; then
    echo "[verify-stack] step 6/6: smoke full-stack (--launch)"
    python3 deploy/scripts/smoke_test.py --launch --with-db --full-stack
  else
    echo "[verify-stack] step 6/6: smoke full-stack (against running services)"
    python3 deploy/scripts/smoke_test.py --with-db --full-stack
  fi
else
  echo "[verify-stack] step 6/6: skipped smoke (RUN_SMOKE=${RUN_SMOKE})"
fi

echo "[verify-stack] OK - suite green + db/migrations verified + clean restart done"
