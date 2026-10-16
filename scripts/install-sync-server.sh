#!/usr/bin/env bash
# Install the CodexBar sync server as a systemd --user service.
#
# The sync server ships in the Rust binary built from native-cli/, not in the
# upstream `codexbar` the widget auto-updates into the managed bin dir (that
# one exits with `Unknown command 'sync-server'`). scripts/codexbar-sync-server.service
# runs bin/codexbar-plasmoid, so the binary has to be put there: pointing the
# unit at a path nothing installs fails with status=203/EXEC, the unit restart
# loops, and every widget on the network sees ECONNREFUSED on the sync port.
#
# Usage:
#   scripts/install-sync-server.sh            # build, install the unit, (re)start it
#   scripts/install-sync-server.sh --no-start # install without touching the running service
#
# The widget points at http://127.0.0.1:8188 by default; the token is printed
# at the end and also stays readable with
#   ~/.local/share/codexbar-plasmoid/bin/codexbar-plasmoid sync-server --show-token

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"

unit_name="codexbar-sync-server.service"
unit_src="$repo_root/scripts/$unit_name"
bin_dir="${XDG_DATA_HOME:-$HOME/.local/share}/codexbar-plasmoid/bin"
target_binary="$bin_dir/codexbar-plasmoid"
unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

start=true
for arg in "$@"; do
  case "$arg" in
    --no-start) start=false ;;
    -h|--help)
      sed -n '2,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to build the sync server." >&2
  exit 1
fi
if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemctl is required to install the user unit." >&2
  exit 1
fi

cargo build --release --manifest-path "$repo_root/native-cli/Cargo.toml"

install -d -m 755 "$bin_dir"
install -m 755 "$repo_root/native-cli/target/release/codexbar-plasmoid" "$target_binary"

install -d -m 755 "$unit_dir"
install -m 644 "$unit_src" "$unit_dir/$unit_name"

systemctl --user daemon-reload
systemctl --user enable "$unit_name" >/dev/null

if [[ "$start" == true ]]; then
  systemctl --user restart "$unit_name"
fi

printf 'Binary: %s\n' "$target_binary"
printf 'Unit:   %s\n' "$unit_dir/$unit_name"
if [[ "$start" == true ]]; then
  systemctl --user --no-pager --lines=0 status "$unit_name" | sed -n '1,6p' || true
fi
printf 'Token:  %s\n' "$("$target_binary" sync-server --show-token)"
printf 'Widget: General -> Sync server, http://127.0.0.1:8188 plus that token.\n'
