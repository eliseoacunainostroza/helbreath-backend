#!/usr/bin/env bash
set -euo pipefail

SYSTEMD_DIR="/etc/systemd/system"
SRC_DIR="$(cd "$(dirname "$0")/../systemd" && pwd)"

sudo cp "${SRC_DIR}"/*.service "${SYSTEMD_DIR}/"
sudo systemctl daemon-reload

echo "[systemd] service files installed"
echo "[systemd] enable with:"
echo "  sudo systemctl enable --now hb-gateway hb-auth hb-world hb-map hb-chat hb-admin-api hb-jobs"
