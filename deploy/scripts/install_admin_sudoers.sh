#!/usr/bin/env bash
set -euo pipefail

SUDOERS_USER="${SUDOERS_USER:-eliseo}"
SUDOERS_FILE="/etc/sudoers.d/hb-admin-api-systemctl"
SYSTEMCTL_BIN="$(command -v systemctl || true)"
JOURNALCTL_BIN="$(command -v journalctl || true)"

if [[ -z "${SYSTEMCTL_BIN}" ]]; then
  echo "[sudoers] no se encontro systemctl en PATH"
  exit 1
fi
if [[ -z "${JOURNALCTL_BIN}" ]]; then
  echo "[sudoers] no se encontro journalctl en PATH"
  exit 1
fi

TMP_FILE="$(mktemp /tmp/hb-sudoers-XXXXXX)"
cleanup() {
  rm -f "${TMP_FILE}"
}
trap cleanup EXIT

cat >"${TMP_FILE}" <<EOF
# Permite al usuario del admin-api gestionar solo servicios hb-* sin prompt.
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-gateway.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-gateway.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-gateway.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-auth.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-auth.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-auth.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-world.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-world.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-world.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-map.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-map.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-map.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-chat.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-chat.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-chat.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-jobs.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-jobs.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-jobs.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} start hb-admin-api.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} restart hb-admin-api.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${SYSTEMCTL_BIN} stop hb-admin-api.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-gateway.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-auth.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-world.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-map.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-chat.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-jobs.service
${SUDOERS_USER} ALL=(root) NOPASSWD: ${JOURNALCTL_BIN} --no-pager -n * -u hb-admin-api.service
EOF

sudo install -m 440 "${TMP_FILE}" "${SUDOERS_FILE}"
sudo visudo -cf "${SUDOERS_FILE}"

echo "[sudoers] instalado: ${SUDOERS_FILE}"
echo "[sudoers] usuario habilitado: ${SUDOERS_USER}"
