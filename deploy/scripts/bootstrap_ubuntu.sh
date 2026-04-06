#!/usr/bin/env bash
set -euo pipefail

HB_DB_NAME="${HB_DB_NAME:-helbreath}"
HB_DB_USER="${HB_DB_USER:-hb}"
HB_DB_PASS="${HB_DB_PASS:-hbpass}"
INSTALL_REDIS="${INSTALL_REDIS:-true}"

sudo apt-get update
sudo apt-get install -y \
  ca-certificates \
  curl \
  git \
  python3 \
  python3-pip \
  build-essential \
  pkg-config \
  libssl-dev \
  postgresql \
  postgresql-contrib

if [[ "${INSTALL_REDIS}" == "true" ]]; then
  sudo apt-get install -y redis-server
fi

if ! command -v rustup >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi

source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable

sudo systemctl enable --now postgresql
if [[ "${INSTALL_REDIS}" == "true" ]]; then
  sudo systemctl enable --now redis-server
fi

if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='${HB_DB_USER}'" | grep -q 1; then
  sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE ROLE ${HB_DB_USER} WITH LOGIN PASSWORD '${HB_DB_PASS}';"
else
  sudo -u postgres psql -v ON_ERROR_STOP=1 -c "ALTER ROLE ${HB_DB_USER} WITH PASSWORD '${HB_DB_PASS}';"
fi

if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='${HB_DB_NAME}'" | grep -q 1; then
  sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${HB_DB_NAME} OWNER ${HB_DB_USER};"
fi

sudo -u postgres psql -v ON_ERROR_STOP=1 -d "${HB_DB_NAME}" \
  -c "GRANT ALL PRIVILEGES ON DATABASE ${HB_DB_NAME} TO ${HB_DB_USER};"

echo "[bootstrap] done"
echo "[bootstrap] database: postgres://${HB_DB_USER}:***@127.0.0.1:5432/${HB_DB_NAME}"
