#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"
package_dir="$CODEXBAR_PACKAGE_DIR"

allow_host_window=0
enable_accessibility=0

usage() {
  cat <<'EOF'
Usage: ./scripts/run-windowed.sh --allow-host-window [--accessibility]

This command opens a real window in the current desktop session. Agent and CI
checks must use ./scripts/run-virtual-plasma.sh or ./scripts/run-config-smoke.sh.

  --allow-host-window  Required explicit acknowledgement of host UI impact
  --accessibility      Keep Qt AT-SPI enabled for an intentional a11y session
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --allow-host-window) allow_host_window=1; shift ;;
    --accessibility) enable_accessibility=1; shift ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ "$allow_host_window" -ne 1 ]]; then
  echo "Refusing to open a host Plasma window without --allow-host-window." >&2
  echo "Use ./scripts/run-virtual-plasma.sh for isolated agent testing." >&2
  exit 2
fi

"$repo_root/scripts/build-native-cli.sh"

if ! command -v plasmawindowed >/dev/null 2>&1; then
  echo "plasmawindowed is required to run the widget in a window." >&2
  exit 1
fi

# Human-only host viewer. Suppress Qt's AT-SPI client unless an intentional
# accessibility session requested it; stale/non-exported host a11y bus addresses
# otherwise produce org.freedesktop.DBus.Error.Disconnected at startup.
if [[ "$enable_accessibility" -ne 1 ]]; then
  export QT_ACCESSIBILITY=0
fi
exec plasmawindowed "$package_dir"
