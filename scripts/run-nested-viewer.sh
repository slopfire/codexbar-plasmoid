#!/usr/bin/env bash
# Nested (windowed) KWin on the *host* compositor — one extra window.
# Prefer ./scripts/run-virtual-plasma.sh for zero host UI interruption.
#
# Usage:
#   ./scripts/run-nested-viewer.sh
#   ./scripts/run-nested-viewer.sh --viewer horizontal

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$repo_root/plasmoid"
plugin_id="org.slopfire.codexbar-plasmoid"

width=1024
height=768
mode="windowed"
formfactor="planar"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --width) width="$2"; shift 2 ;;
    --height) height="$2"; shift 2 ;;
    --viewer)
      mode="viewer"
      if [[ $# -ge 2 && "$2" != --* ]]; then formfactor="$2"; shift 2; else shift; fi
      ;;
    -h|--help)
      sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

if ! command -v kwin_wayland >/dev/null 2>&1; then
  echo "kwin_wayland is required." >&2
  exit 1
fi

socket_name="wayland-codexbar-nested-$$"
log_file="/tmp/codexbar-nested-${socket_name}.log"
runner="$(mktemp /tmp/codexbar-nested-runner.XXXXXX)"
cleanup() { rm -f "$runner"; }
trap cleanup EXIT

{
  echo '#!/usr/bin/env bash'
  echo 'set -euo pipefail'
  if [[ "$mode" == "viewer" ]]; then
    printf 'exec plasmoidviewer -a %q -f %q' "$plugin_id" "$formfactor"
    case "$formfactor" in
      horizontal) echo ' -l bottomedge' ;;
      vertical) echo ' -l leftedge' ;;
      *) echo ;;
    esac
  else
    printf 'exec plasmawindowed %q\n' "$package_dir"
  fi
} >"$runner"
chmod +x "$runner"

echo "Nested KWin ${width}x${height}; runner: $runner"
echo "Log: $log_file"
# Nested windowed compositor as a single client window of the host session.
exec kwin_wayland \
  --width "$width" \
  --height "$height" \
  --socket "$socket_name" \
  --no-lockscreen \
  --exit-with-session "$runner" \
  >"$log_file" 2>&1
