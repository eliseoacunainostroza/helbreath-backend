#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "uso: bash deploy/scripts/restore_postgres.sh <ruta_backup.dump|ruta_backup.dump.gz>"
  exit 1
fi

BACKUP_FILE="$1"

if [[ -z "${HB_DATABASE_URL:-}" ]]; then
  echo "[restore-postgres] define HB_DATABASE_URL antes de ejecutar"
  exit 1
fi

if ! command -v pg_restore >/dev/null 2>&1; then
  echo "[restore-postgres] pg_restore no está disponible"
  exit 1
fi

if [[ ! -f "${BACKUP_FILE}" ]]; then
  echo "[restore-postgres] no existe el archivo: ${BACKUP_FILE}"
  exit 1
fi

TMP_FILE=""
if [[ "${BACKUP_FILE}" == *.gz ]]; then
  TMP_FILE="$(mktemp /tmp/helbreath_restore_XXXXXX.dump)"
  gzip -dc "${BACKUP_FILE}" > "${TMP_FILE}"
  BACKUP_FILE="${TMP_FILE}"
fi

cleanup() {
  if [[ -n "${TMP_FILE}" && -f "${TMP_FILE}" ]]; then
    rm -f "${TMP_FILE}"
  fi
}
trap cleanup EXIT

echo "[restore-postgres] restaurando ${BACKUP_FILE} en ${HB_DATABASE_URL}"
pg_restore "${BACKUP_FILE}" \
  --dbname="${HB_DATABASE_URL}" \
  --clean \
  --if-exists \
  --no-owner \
  --no-privileges \
  --jobs=2

echo "[restore-postgres] restauración completada"
