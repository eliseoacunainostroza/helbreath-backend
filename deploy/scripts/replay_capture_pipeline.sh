#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

PROTOCOL="legacy_v382"
LISTEN="${CAPTURE_LISTEN:-127.0.0.1:3848}"
UPSTREAM="${CAPTURE_UPSTREAM:-127.0.0.1:2848}"
PHASE="in_world"
ORIGIN="capture"
INPUT=""
OUTPUT=""
RUN_TESTS=1
RUN_STRICT=0

usage() {
  cat <<'USAGE'
Usage:
  bash deploy/scripts/replay_capture_pipeline.sh [options]

Options:
  --protocol legacy_v382|modern_v400   Protocol for generated cases (default: legacy_v382)
  --listen HOST:PORT                    Local capture proxy bind (default: 127.0.0.1:3848)
  --upstream HOST:PORT                  Gateway TCP bind (default: 127.0.0.1:2848)
  --input PATH                          Existing replay .bin (skip live capture)
  --output PATH                         Capture output path (default: tmp/replay_capture_<protocol>_<ts>.bin)
  --no-tests                            Skip cargo replay tests after merge
  --strict                              Fail if required real coverage is incomplete for selected protocol
  -h|--help                             Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --protocol)
      PROTOCOL="${2:-}"
      shift 2
      ;;
    --listen)
      LISTEN="${2:-}"
      shift 2
      ;;
    --upstream)
      UPSTREAM="${2:-}"
      shift 2
      ;;
    --input)
      INPUT="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT="${2:-}"
      shift 2
      ;;
    --no-tests)
      RUN_TESTS=0
      shift
      ;;
    --strict)
      RUN_STRICT=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[replay-capture-pipeline] unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$PROTOCOL" in
  legacy_v382|modern_v400) ;;
  *)
    echo "[replay-capture-pipeline] invalid --protocol: $PROTOCOL" >&2
    exit 2
    ;;
esac

if [[ -z "$OUTPUT" ]]; then
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  OUTPUT="tmp/replay_capture_${PROTOCOL}_${ts}.bin"
fi

mkdir -p "$(dirname "$OUTPUT")"

if [[ -z "$INPUT" ]]; then
  echo "[replay-capture-pipeline] live capture mode"
  echo "[replay-capture-pipeline] protocol=$PROTOCOL listen=$LISTEN upstream=$UPSTREAM"
  echo "[replay-capture-pipeline] output=$OUTPUT"
  echo "[replay-capture-pipeline] point your client to $LISTEN, play the full flow, then Ctrl+C"
  python3 deploy/scripts/capture_replay_frames.py \
    --listen "$LISTEN" \
    --upstream "$UPSTREAM" \
    --output "$OUTPUT"
  INPUT="$OUTPUT"
fi

if [[ ! -f "$INPUT" ]]; then
  echo "[replay-capture-pipeline] input file not found: $INPUT" >&2
  exit 1
fi

if [[ ! -s "$INPUT" ]]; then
  echo "[replay-capture-pipeline] input file is empty: $INPUT" >&2
  exit 1
fi

echo "[replay-capture-pipeline] generating replay cases from: $INPUT"
python3 deploy/scripts/replay_fixture_from_bin.py \
  --input "$INPUT" \
  --output crates/net/tests/fixtures/replay_cases.generated.json \
  --phase "$PHASE" \
  --protocol-version "$PROTOCOL" \
  --expect-mode opcode_command \
  --auto-phase \
  --origin "$ORIGIN"

echo "[replay-capture-pipeline] merging + splitting fixtures"
python3 deploy/scripts/replay_merge_cases.py \
  --base crates/net/tests/fixtures/replay_cases.json \
  --incoming crates/net/tests/fixtures/replay_cases.generated.json \
  --output crates/net/tests/fixtures/replay_cases.json

python3 deploy/scripts/replay_split_cases.py \
  --input crates/net/tests/fixtures/replay_cases.json \
  --legacy-output crates/net/tests/fixtures/replay_cases_legacy_v382.json \
  --modern-output crates/net/tests/fixtures/replay_cases_modern_v400.json

echo "[replay-capture-pipeline] updating opcode reports"
python3 deploy/scripts/replay_opcode_report.py \
  --input crates/net/tests/fixtures/replay_cases.json \
  --markdown-output docs/protocol_opcode_matrix.md \
  --json-output docs/protocol_opcode_matrix.json
python3 deploy/scripts/generate_net_parity_checklist.py \
  --input docs/protocol_opcode_matrix.json \
  --output docs/net_legacy_parity_checklist.md
python3 deploy/scripts/replay_capture_todo.py \
  --input docs/protocol_opcode_matrix.json \
  --output docs/protocol_capture_todo.md \
  --protocols "$PROTOCOL"
python3 deploy/scripts/generate_protocol_capture_playbook.py \
  --input docs/protocol_opcode_matrix.json \
  --output docs/protocol_capture_playbook.md \
  --protocols "$PROTOCOL"

if [[ "$RUN_TESTS" -eq 1 ]]; then
  echo "[replay-capture-pipeline] running replay tests"
  cargo test -p net replay_cases_json_fixture -- --nocapture
  if [[ "$PROTOCOL" == "legacy_v382" ]]; then
    cargo test -p net replay_cases_legacy_v382_fixture -- --nocapture
  else
    cargo test -p net replay_cases_modern_v400_fixture -- --nocapture
  fi
fi

if [[ "$RUN_STRICT" -eq 1 ]]; then
  echo "[replay-capture-pipeline] running strict real-parity check for $PROTOCOL"
  python3 deploy/scripts/replay_opcode_report.py \
    --input crates/net/tests/fixtures/replay_cases.json \
    --markdown-output docs/protocol_opcode_matrix.md \
    --json-output docs/protocol_opcode_matrix.json \
    --fail-on-real-gaps \
    --real-required-protocols "$PROTOCOL"
fi

echo "[replay-capture-pipeline] done"
echo "[replay-capture-pipeline] next: inspect docs/protocol_opcode_matrix.md (faltantes_real)"
echo "[replay-capture-pipeline] checklist: docs/net_legacy_parity_checklist.md"
echo "[replay-capture-pipeline] todo: docs/protocol_capture_todo.md"
echo "[replay-capture-pipeline] playbook: docs/protocol_capture_playbook.md"
