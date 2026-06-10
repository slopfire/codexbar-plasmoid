#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$repo_root/plasmoid"

"$repo_root/scripts/build-native-cli.sh"

if ! command -v plasmawindowed >/dev/null 2>&1; then
  echo "plasmawindowed is required to run the widget in a window." >&2
  exit 1
fi

exec plasmawindowed "$package_dir"
