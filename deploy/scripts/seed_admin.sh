#!/usr/bin/env bash
set -euo pipefail

DB_URL="${HB_DATABASE_URL:-postgres://hb:hbpass@127.0.0.1:5432/helbreath}"
ADMIN_EMAIL="${HB_BOOTSTRAP_ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${HB_BOOTSTRAP_ADMIN_PASSWORD:-change_me_now}"
# You can provide this directly, or let this script derive it from HB_BOOTSTRAP_ADMIN_PASSWORD.
ADMIN_PASSWORD_HASH="${HB_BOOTSTRAP_ADMIN_PASSWORD_HASH:-}"

if [[ -z "${ADMIN_PASSWORD_HASH}" ]]; then
  if ! command -v python3 >/dev/null 2>&1; then
    echo "[seed] python3 no encontrado y HB_BOOTSTRAP_ADMIN_PASSWORD_HASH no fue definido."
    exit 1
  fi
  if ! ADMIN_PASSWORD_HASH="$(
    python3 - "${ADMIN_PASSWORD}" <<'PY'
import sys
from argon2 import PasswordHasher

password = sys.argv[1]
print(PasswordHasher().hash(password))
PY
  )"; then
    echo "[seed] no se pudo generar hash Argon2 automaticamente."
    echo "[seed] instale python3-argon2 o exporte HB_BOOTSTRAP_ADMIN_PASSWORD_HASH manualmente."
    exit 1
  fi
fi

if [[ "${ADMIN_PASSWORD_HASH}" != \$argon2* ]]; then
  echo "[seed] HB_BOOTSTRAP_ADMIN_PASSWORD_HASH invalido (debe ser Argon2, iniciar con \$argon2)."
  exit 1
fi

psql "${DB_URL}" -v ON_ERROR_STOP=1 <<SQL
INSERT INTO admin_users(email, password_hash)
VALUES ('${ADMIN_EMAIL}', '${ADMIN_PASSWORD_HASH}')
ON CONFLICT (email) DO UPDATE SET password_hash = EXCLUDED.password_hash;

INSERT INTO admin_user_roles(admin_user_id, role_id)
SELECT u.id, r.id
FROM admin_users u
JOIN admin_roles r ON r.code = 'superadmin'
WHERE u.email = '${ADMIN_EMAIL}'
ON CONFLICT DO NOTHING;
SQL

echo "[seed] bootstrap admin ready: ${ADMIN_EMAIL}"
