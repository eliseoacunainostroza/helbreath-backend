#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

VERSION="${1:-${VERSION:-}}"
PUSH_TAG="${PUSH_TAG:-0}"
ALLOW_DIRTY="${ALLOW_DIRTY:-0}"

usage() {
  cat <<'USAGE'
Uso:
  VERSION=v0.1.0 bash deploy/scripts/tag_release.sh
  bash deploy/scripts/tag_release.sh v0.1.0

Variables opcionales:
  PUSH_TAG=1     # hace push del tag al remoto origin
  ALLOW_DIRTY=1  # permite crear tag con working tree sucio (no recomendado)
USAGE
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "[tag-release] falta comando requerido: ${cmd}"
    exit 1
  fi
}

main() {
  if [[ -z "${VERSION}" ]]; then
    usage
    exit 1
  fi

  if [[ ! "${VERSION}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z]+)*$ ]]; then
    echo "[tag-release] version invalida: ${VERSION}"
    echo "[tag-release] formato esperado: vMAJOR.MINOR.PATCH (ej: v0.1.0)"
    exit 1
  fi

  require_cmd git
  require_cmd date

  if ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo "[tag-release] este directorio no es un repositorio git"
    exit 1
  fi

  if [[ "${ALLOW_DIRTY}" != "1" ]]; then
    if [[ -n "$(git status --porcelain)" ]]; then
      echo "[tag-release] working tree no esta limpio. Commit/stash antes de tagear."
      echo "[tag-release] use ALLOW_DIRTY=1 solo si sabes exactamente lo que haces."
      exit 1
    fi
  fi

  if git rev-parse "${VERSION}" >/dev/null 2>&1; then
    echo "[tag-release] el tag ya existe: ${VERSION}"
    exit 1
  fi

  local now_utc
  now_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  local branch
  branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"

  git tag -a "${VERSION}" -m "Helbreath backend ${VERSION}

Fecha UTC: ${now_utc}
Branch: ${branch}
Checklist: release_gate + soak + slo-check"

  echo "[tag-release] tag creado: ${VERSION}"
  echo "[tag-release] siguiente paso sugerido:"
  echo "  git push origin ${VERSION}"

  if [[ "${PUSH_TAG}" == "1" ]]; then
    git push origin "${VERSION}"
    echo "[tag-release] tag publicado en origin/${VERSION}"
  fi
}

main "$@"
