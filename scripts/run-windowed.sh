#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"
package_dir="$CODEXBAR_PACKAGE_DIR"

"$repo_root/scripts/build-native-cli.sh"

if ! command -v plasmawindowed >/dev/null 2>&1; then
  echo "plasmawindowed is required to run the widget in a window." >&2
  exit 1
fi

# Prefer virtual for agents: ./scripts/run-virtual-plasma.sh
exec plasmawindowed "$package_dir"
