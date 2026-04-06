#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OBS_DIR="${ROOT_DIR}/deploy/observability"

if ! command -v docker >/dev/null 2>&1; then
  echo "[observability] docker is required"
  exit 1
fi

if docker compose version >/dev/null 2>&1; then
  COMPOSE_CMD=(docker compose)
elif command -v docker-compose >/dev/null 2>&1; then
  COMPOSE_CMD=(docker-compose)
else
  echo "[observability] docker compose is required (plugin v2 or docker-compose standalone)"
  exit 1
fi

echo "[observability] using compose file: ${OBS_DIR}/docker-compose.observability.yml"
"${COMPOSE_CMD[@]}" -f "${OBS_DIR}/docker-compose.observability.yml" up -d

echo "[observability] Prometheus: http://127.0.0.1:9090"
echo "[observability] Grafana   : http://127.0.0.1:3000 (admin/admin)"
echo "[observability] Grafana dashboard: Helbreath/Helbreath Backend - Overview"
echo "[observability] Prometheus alert rules loaded from deploy/observability/alerts.yml"
