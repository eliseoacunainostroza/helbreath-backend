#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BACKUP_DIR="${BACKUP_DIR:-${ROOT_DIR}/backups/postgres}"

latest="$(ls -1t "${BACKUP_DIR}"/helbreath_*.dump* 2>/dev/null | head -n 1 || true)"
if [[ -z "${latest}" ]]; then
  echo "[restore-latest] no se encontraron backups en ${BACKUP_DIR}"
  exit 1
fi

echo "[restore-latest] restaurando backup más reciente: ${latest}"
bash "${ROOT_DIR}/deploy/scripts/restore_postgres.sh" "${latest}"
