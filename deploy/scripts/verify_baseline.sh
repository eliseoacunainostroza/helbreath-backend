#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

RUN_FMT="${RUN_FMT:-1}"
RUN_CLIPPY="${RUN_CLIPPY:-1}"
RUN_TEST="${RUN_TEST:-1}"
RUN_SMOKE="${RUN_SMOKE:-1}"
RUN_FULL_STACK="${RUN_FULL_STACK:-0}"
SMOKE_SETUP="${SMOKE_SETUP:-0}"
RUN_DOCS_HTML="${RUN_DOCS_HTML:-1}"
RUN_PARITY_REPORTS="${RUN_PARITY_REPORTS:-1}"
RUN_PARITY_STRICT="${RUN_PARITY_STRICT:-0}"
RUN_SEED_ADMIN="${RUN_SEED_ADMIN:-1}"

# Evita errores "Text file busy" frecuentes en carpetas compartidas (VirtualBox /mnt/*).
# Se puede sobrescribir externamente si se requiere otro path.
if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
  export CARGO_TARGET_DIR="/var/tmp/helbreath-cargo-target"
fi
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
mkdir -p "${CARGO_TARGET_DIR}"

echo "[baseline] root=${ROOT_DIR}"
echo "[baseline] cargo_target_dir=${CARGO_TARGET_DIR}"
echo "[baseline] cargo_incremental=${CARGO_INCREMENTAL}"
echo "[baseline] cargo_build_jobs=${CARGO_BUILD_JOBS}"

if [[ "${RUN_FMT}" == "1" ]]; then
  echo "[baseline] cargo fmt --all -- --check"
  cargo fmt --all -- --check
fi

if [[ "${RUN_CLIPPY}" == "1" ]]; then
  echo "[baseline] cargo clippy --workspace --all-targets -- -D warnings"
  cargo clippy --workspace --all-targets -- -D warnings
fi

if [[ "${RUN_TEST}" == "1" ]]; then
  echo "[baseline] cargo test --workspace"
  cargo test --workspace
fi

if [[ "${RUN_PARITY_REPORTS}" == "1" ]]; then
  echo "[baseline] refreshing protocol parity reports"
  python3 deploy/scripts/replay_opcode_report.py \
    --input crates/net/tests/fixtures/replay_cases.json \
    --markdown-output docs/protocol_opcode_matrix.md \
    --json-output docs/protocol_opcode_matrix.json
  python3 deploy/scripts/replay_capture_todo.py \
    --input docs/protocol_opcode_matrix.json \
    --output docs/protocol_capture_todo.md \
    --protocols "${REAL_PARITY_PROTOCOLS:-legacy_v382,modern_v400}"
  python3 deploy/scripts/generate_protocol_capture_playbook.py \
    --input docs/protocol_opcode_matrix.json \
    --output docs/protocol_capture_playbook.md \
    --protocols "${REAL_PARITY_PROTOCOLS:-legacy_v382,modern_v400}"
  python3 deploy/scripts/generate_net_parity_checklist.py \
    --input docs/protocol_opcode_matrix.json \
    --output docs/net_legacy_parity_checklist.md

  if [[ "${RUN_PARITY_STRICT}" == "1" ]]; then
    echo "[baseline] enforcing real protocol parity gate"
    python3 deploy/scripts/replay_opcode_report.py \
      --input crates/net/tests/fixtures/replay_cases.json \
      --markdown-output docs/protocol_opcode_matrix.md \
      --json-output docs/protocol_opcode_matrix.json \
      --fail-on-real-gaps \
      --real-required-protocols "${REAL_PARITY_PROTOCOLS:-legacy_v382,modern_v400}"
  fi
fi

if [[ "${RUN_DOCS_HTML}" == "1" ]]; then
  echo "[baseline] python3 deploy/scripts/generate_docs_html.py"
  python3 deploy/scripts/generate_docs_html.py
fi

if [[ "${RUN_SMOKE}" == "1" ]]; then
  if [[ "${RUN_SEED_ADMIN}" == "1" ]]; then
    echo "[baseline] bash deploy/scripts/seed_admin.sh"
    bash deploy/scripts/seed_admin.sh
  fi

  smoke_cmd=(python3 deploy/scripts/smoke_test.py --launch --with-db)
  if [[ "${SMOKE_SETUP}" == "1" ]]; then
    smoke_cmd+=(--setup)
  fi
  if [[ "${RUN_FULL_STACK}" == "1" ]]; then
    smoke_cmd+=(--full-stack)
  fi

  echo "[baseline] ${smoke_cmd[*]}"
  "${smoke_cmd[@]}"
fi

echo "[baseline] ok"
