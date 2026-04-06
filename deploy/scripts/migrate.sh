#!/usr/bin/env bash
set -euo pipefail

DB_URL="${HB_DATABASE_URL:-postgres://hb:hbpass@127.0.0.1:5432/helbreath}"

if ! command -v psql >/dev/null 2>&1; then
  echo "[migrate] psql is required"
  exit 1
fi

for f in $(ls migrations/*.sql | sort); do
  echo "[migrate] applying $f"
  psql "${DB_URL}" -v ON_ERROR_STOP=1 -f "$f"
done

echo "[migrate] done"
