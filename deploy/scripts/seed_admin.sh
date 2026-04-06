#!/usr/bin/env bash
set -euo pipefail

DB_URL="${HB_DATABASE_URL:-postgres://hb:hbpass@127.0.0.1:5432/helbreath}"
ADMIN_EMAIL="${HB_BOOTSTRAP_ADMIN_EMAIL:-admin@localhost}"
# You can generate this with: python3 - <<'PY' ; import argon2 ; print(argon2.PasswordHasher().hash("change_me_now")) ; PY
ADMIN_PASSWORD_HASH="${HB_BOOTSTRAP_ADMIN_PASSWORD_HASH:-}"

if [[ -z "${ADMIN_PASSWORD_HASH}" ]]; then
  echo "[seed] HB_BOOTSTRAP_ADMIN_PASSWORD_HASH is required (argon2 hash)."
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
