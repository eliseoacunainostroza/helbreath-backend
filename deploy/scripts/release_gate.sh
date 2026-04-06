#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

START_OBSERVABILITY="${START_OBSERVABILITY:-1}"
RUN_MIGRATIONS="${RUN_MIGRATIONS:-1}"
RUN_FULL_STACK="${RUN_FULL_STACK:-1}"
RUN_SOAK="${RUN_SOAK:-0}"
SOAK_ITERATIONS="${SOAK_ITERATIONS:-3}"
SOAK_DELAY_SECONDS="${SOAK_DELAY_SECONDS:-3}"
RUN_SLO_CHECK="${RUN_SLO_CHECK:-1}"
SLO_MIN_ITERATIONS="${SLO_MIN_ITERATIONS:-3}"
SLO_MAX_FAILED_ITERATIONS="${SLO_MAX_FAILED_ITERATIONS:-0}"
SLO_MIN_PASS_RATE="${SLO_MIN_PASS_RATE:-100}"
SLO_MAX_AVG_ITERATION_SECONDS="${SLO_MAX_AVG_ITERATION_SECONDS:-30}"
SLO_MAX_P95_ITERATION_SECONDS="${SLO_MAX_P95_ITERATION_SECONDS:-35}"
RELEASE_GATE_STOP_SYSTEMD="${RELEASE_GATE_STOP_SYSTEMD:-0}"
RELEASE_GATE_KILL_SMOKE_PROCS="${RELEASE_GATE_KILL_SMOKE_PROCS:-0}"

readonly HB_SYSTEMD_UNITS=(
  "hb-gateway"
  "hb-auth"
  "hb-world"
  "hb-map"
  "hb-chat"
  "hb-jobs"
  "hb-admin-api"
)

readonly HB_SMOKE_PORTS=(
  "8080"
  "7101"
  "7080"
  "7201"
  "7301"
  "7401"
  "7501"
)

log() {
  echo "[release-gate] $*"
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "[release-gate] falta comando requerido: ${cmd}"
    exit 1
  fi
}

curl_retry() {
  local url="$1"
  local attempts="${2:-20}"
  local wait_seconds="${3:-1}"
  local i=1

  while (( i <= attempts )); do
    if curl -fsS "${url}" >/dev/null 2>&1; then
      return 0
    fi
    sleep "${wait_seconds}"
    ((i++))
  done

  echo "[release-gate] no se pudo consultar ${url} tras ${attempts} intentos"
  return 1
}

check_observability() {
  log "validando health de Prometheus y Grafana"
  curl_retry "http://127.0.0.1:9090/-/ready"
  curl_retry "http://127.0.0.1:3000/api/health"

  local rules_file
  rules_file="$(mktemp /tmp/hb-rules-XXXXXX.json)"
  curl -fsS "http://127.0.0.1:9090/api/v1/rules" >"${rules_file}"
  python3 - "${rules_file}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fp:
    payload = json.load(fp)
status = payload.get("status")
groups = payload.get("data", {}).get("groups", [])
if status != "success":
    raise SystemExit(f"estado de reglas inesperado: {status!r}")
if len(groups) < 1:
    raise SystemExit("Prometheus no cargo grupos de reglas")
print(f"[release-gate] reglas Prometheus: status={status}, groups={len(groups)}")
PY
  rm -f "${rules_file}"
}

check_docs_spanish() {
  log "validando documentacion HTML en espanol"
  python3 deploy/scripts/generate_docs_html.py >/dev/null
  grep -q '<html lang="es">' docs/html/index.html
  grep -q "Documentos Disponibles" docs/html/index.html
}

list_conflicting_ports() {
  if ! command -v ss >/dev/null 2>&1; then
    return 0
  fi

  local bound_ports
  bound_ports="$(ss -ltnH | awk '{print $4}' | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p' | sort -u)"
  for port in "${HB_SMOKE_PORTS[@]}"; do
    if grep -qx "${port}" <<<"${bound_ports}"; then
      echo "${port}"
    fi
  done
}

preflight_runtime_conflicts() {
  local active_units=()
  if command -v systemctl >/dev/null 2>&1; then
    for unit in "${HB_SYSTEMD_UNITS[@]}"; do
      if systemctl is-active --quiet "${unit}.service"; then
        active_units+=("${unit}.service")
      fi
    done
  fi

  if (( ${#active_units[@]} > 0 )); then
    if [[ "${RELEASE_GATE_STOP_SYSTEMD}" == "1" ]]; then
      log "deteniendo servicios systemd hb-* activos: ${active_units[*]}"
      sudo systemctl stop "${active_units[@]}"
    else
      echo "[release-gate] conflicto: servicios hb-* activos detectados: ${active_units[*]}"
      echo "[release-gate] para evitar choque de puertos con smoke --launch, ejecute:"
      echo "  sudo systemctl stop ${active_units[*]}"
      echo "[release-gate] o use RELEASE_GATE_STOP_SYSTEMD=1"
      exit 1
    fi
  fi

  local cargo_hb_count=0
  local smoke_count=0
  if command -v pgrep >/dev/null 2>&1; then
    cargo_hb_count="$(pgrep -f "cargo run -p hb-" | wc -l || true)"
    smoke_count="$(pgrep -f "smoke_test.py" | wc -l || true)"
  fi
  if [[ "${cargo_hb_count}" != "0" || "${smoke_count}" != "0" ]]; then
    if [[ "${RELEASE_GATE_KILL_SMOKE_PROCS}" == "1" ]]; then
      log "terminando procesos previos de smoke/cargo run"
      pkill -f "cargo run -p hb-" || true
      pkill -f "smoke_test.py" || true
    else
      echo "[release-gate] conflicto: procesos previos detectados (cargo_hb=${cargo_hb_count}, smoke=${smoke_count})"
      echo "[release-gate] ejecute:"
      echo "  pkill -f \"cargo run -p hb-\" || true"
      echo "  pkill -f \"smoke_test.py\" || true"
      echo "[release-gate] o use RELEASE_GATE_KILL_SMOKE_PROCS=1"
      exit 1
    fi
  fi

  local conflicting_ports=()
  while IFS= read -r port; do
    [[ -n "${port}" ]] && conflicting_ports+=("${port}")
  done < <(list_conflicting_ports || true)

  if (( ${#conflicting_ports[@]} > 0 )); then
    echo "[release-gate] advertencia: puertos de servicios ya en uso: ${conflicting_ports[*]}"
    echo "[release-gate] si el baseline falla por conflicto, revise: ss -ltnp | egrep ':(8080|7101|7080|7201|7301|7401|7501)\\b'"
  fi
}

main() {
  log "root=${ROOT_DIR}"
  require_cmd cargo
  require_cmd python3
  require_cmd curl

  preflight_runtime_conflicts

  if [[ "${RUN_MIGRATIONS}" == "1" ]]; then
    log "aplicando migraciones"
    bash deploy/scripts/migrate.sh
  fi

  if [[ "${START_OBSERVABILITY}" == "1" ]]; then
    require_cmd docker
    log "levantando stack de observabilidad"
    bash deploy/scripts/run_observability.sh >/dev/null
  fi

  check_observability
  check_docs_spanish

  log "ejecutando baseline completo"
  RUN_FULL_STACK="${RUN_FULL_STACK}" RUN_DOCS_HTML=1 bash deploy/scripts/verify_baseline.sh

  if [[ "${RUN_SOAK}" == "1" ]]; then
    log "ejecutando soak de estabilidad (${SOAK_ITERATIONS} iteraciones)"
    local soak_reports_dir=".smoke/reports"
    mkdir -p "${soak_reports_dir}"
    local soak_ts
    soak_ts="$(date -u +%Y%m%dT%H%M%SZ)"
    local soak_json="${soak_reports_dir}/soak_${soak_ts}.json"
    python3 deploy/scripts/soak_test.py \
      --iterations "${SOAK_ITERATIONS}" \
      --delay-seconds "${SOAK_DELAY_SECONDS}" \
      --full-stack \
      --with-db \
      --stop-on-fail \
      --json-output "${soak_json}"

    log "generando reporte de tendencia soak"
    python3 deploy/scripts/soak_report.py \
      --input-glob "${soak_reports_dir}/soak_*.json" \
      --output-md "docs/soak_stability.md"
    python3 deploy/scripts/generate_docs_html.py >/dev/null

    if [[ "${RUN_SLO_CHECK}" == "1" ]]; then
      log "evaluando umbrales SLO de release"
      python3 deploy/scripts/evaluate_release_slo.py \
        --input "${soak_json}" \
        --min-iterations "${SLO_MIN_ITERATIONS}" \
        --max-failed-iterations "${SLO_MAX_FAILED_ITERATIONS}" \
        --min-pass-rate "${SLO_MIN_PASS_RATE}" \
        --max-avg-iteration-seconds "${SLO_MAX_AVG_ITERATION_SECONDS}" \
        --max-p95-iteration-seconds "${SLO_MAX_P95_ITERATION_SECONDS}"
    fi
  fi

  log "OK: release gate aprobado"
}

main "$@"
