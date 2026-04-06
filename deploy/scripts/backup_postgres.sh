#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

OUT_DIR="${OUT_DIR:-${ROOT_DIR}/backups/postgres}"
COMPRESS="${COMPRESS:-1}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

if [[ -z "${HB_DATABASE_URL:-}" ]]; then
  echo "[backup-postgres] define HB_DATABASE_URL antes de ejecutar"
  exit 1
fi

if ! command -v pg_dump >/dev/null 2>&1; then
  echo "[backup-postgres] pg_dump no está disponible"
  exit 1
fi

mkdir -p "${OUT_DIR}"
OUT_FILE="${OUT_DIR}/helbreath_${TIMESTAMP}.dump"

echo "[backup-postgres] generando backup en ${OUT_FILE}"
pg_dump "${HB_DATABASE_URL}" \
  --format=custom \
  --no-owner \
  --no-privileges \
  --file "${OUT_FILE}"

if [[ "${COMPRESS}" == "1" ]]; then
  gzip -f "${OUT_FILE}"
  OUT_FILE="${OUT_FILE}.gz"
fi

echo "[backup-postgres] listo: ${OUT_FILE}"
