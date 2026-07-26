# shellcheck shell=bash
# Shared constants for CodexBar plasmoid scripts. Source from other scripts:
#   # shellcheck source=lib/repo-env.sh
#   source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib/repo-env.sh"
#
# Or from repo root:
#   source scripts/lib/repo-env.sh

if [[ -z "${CODEXBAR_REPO_ROOT:-}" ]]; then
  if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    _repo_env_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    CODEXBAR_REPO_ROOT="$(cd -- "$_repo_env_dir/../.." && pwd)"
  else
    CODEXBAR_REPO_ROOT="$(pwd)"
  fi
  unset _repo_env_dir
fi

export CODEXBAR_REPO_ROOT
export CODEXBAR_PLUGIN_ID="${CODEXBAR_PLUGIN_ID:-org.slopfire.codexbar-plasmoid}"
export CODEXBAR_PACKAGE_DIR="${CODEXBAR_PACKAGE_DIR:-$CODEXBAR_REPO_ROOT/plasmoid}"
export CODEXBAR_MOCK_DIR="${CODEXBAR_MOCK_DIR:-/tmp/codexbar-plasma-mock}"
export CODEXBAR_VIRTUAL_LATEST_DIR="${CODEXBAR_VIRTUAL_LATEST_DIR:-/tmp/codexbar-virtual-latest}"

# Old package IDs the installer removes
CODEXBAR_OLD_PLUGIN_IDS=(org.kde.codexbar org.splazma.codexbar)
