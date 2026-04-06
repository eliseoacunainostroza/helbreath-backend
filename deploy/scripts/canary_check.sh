#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

RUN_BASELINE="${RUN_BASELINE:-0}"
RUN_SOAK="${RUN_SOAK:-1}"
RUN_SLO_CHECK="${RUN_SLO_CHECK:-1}"
SOAK_ITERATIONS="${SOAK_ITERATIONS:-3}"
SOAK_DELAY_SECONDS="${SOAK_DELAY_SECONDS:-3}"
SOAK_REPORT_GLOB="${SOAK_REPORT_GLOB:-.smoke/reports/soak_*.json}"

SLO_MIN_ITERATIONS="${SLO_MIN_ITERATIONS:-3}"
SLO_MAX_FAILED_ITERATIONS="${SLO_MAX_FAILED_ITERATIONS:-0}"
SLO_MIN_PASS_RATE="${SLO_MIN_PASS_RATE:-100}"
SLO_MAX_AVG_ITERATION_SECONDS="${SLO_MAX_AVG_ITERATION_SECONDS:-30}"
SLO_MAX_P95_ITERATION_SECONDS="${SLO_MAX_P95_ITERATION_SECONDS:-35}"

log() {
  echo "[canary-check] $*"
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "[canary-check] falta comando requerido: ${cmd}"
    exit 1
  fi
}

main() {
  require_cmd python3
  require_cmd bash

  log "root=${ROOT_DIR}"

  if [[ "${RUN_BASELINE}" == "1" ]]; then
    log "ejecutando baseline completo"
    RUN_FULL_STACK=1 bash deploy/scripts/verify_baseline.sh
  fi

  log "smoke canary: health"
  python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only health --verbose

  log "smoke canary: gateway tcp legacy"
  python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only gateway.tcp.route-flow --verbose

  log "smoke canary: gateway tcp modern"
  python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db --only gateway.tcp.route-flow.modern --verbose

  if [[ "${RUN_SOAK}" == "1" ]]; then
    mkdir -p .smoke/reports
    local ts
    ts="$(date -u +%Y%m%dT%H%M%SZ)"
    local soak_json=".smoke/reports/soak_canary_${ts}.json"
    log "soak canary -> ${soak_json}"
    python3 deploy/scripts/soak_test.py \
      --iterations "${SOAK_ITERATIONS}" \
      --delay-seconds "${SOAK_DELAY_SECONDS}" \
      --full-stack \
      --with-db \
      --json-output "${soak_json}"

    python3 deploy/scripts/soak_report.py \
      --input-glob "${SOAK_REPORT_GLOB}" \
      --output-md "docs/soak_stability.md"

    if [[ "${RUN_SLO_CHECK}" == "1" ]]; then
      log "evaluando SLO de canary"
      python3 deploy/scripts/evaluate_release_slo.py \
        --input "${soak_json}" \
        --min-iterations "${SLO_MIN_ITERATIONS}" \
        --max-failed-iterations "${SLO_MAX_FAILED_ITERATIONS}" \
        --min-pass-rate "${SLO_MIN_PASS_RATE}" \
        --max-avg-iteration-seconds "${SLO_MAX_AVG_ITERATION_SECONDS}" \
        --max-p95-iteration-seconds "${SLO_MAX_P95_ITERATION_SECONDS}"
    fi
  fi

  python3 deploy/scripts/generate_docs_html.py >/dev/null
  log "OK"
}

main "$@"
