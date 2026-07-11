#!/usr/bin/env bash
# Build a store-ready .plasmoid archive (zip of package contents).
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$repo_root/plasmoid"
metadata="$package_dir/metadata.json"
dist_dir="$repo_root/dist"

if ! command -v zip >/dev/null 2>&1; then
  echo "zip is required to package the Plasma widget." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to read package metadata." >&2
  exit 1
fi

# Ensure the bundled native helper is present and current.
"$repo_root/scripts/build-native-cli.sh"

# Release archives must include executable helpers.
chmod +x \
  "$package_dir/contents/code/codexbar-plasmoid" \
  "$package_dir/contents/code/codexbar-plasmoid-helper.mjs" \
  "$package_dir/contents/code/codexbar-cli-updater.mjs"

readarray -t meta < <(python3 - "$metadata" <<'PY'
import json, sys
meta = json.load(open(sys.argv[1]))
plugin = meta["KPlugin"]
print(plugin["Id"])
print(plugin["Version"])
print(plugin.get("Name", plugin["Id"]))
PY
)
plugin_id="${meta[0]}"
version="${meta[1]}"
name="${meta[2]}"
short_name="${plugin_id##*.}"
filename="${short_name}-v${version}-plasma6.plasmoid"

mkdir -p "$dist_dir"
output="$dist_dir/$filename"
rm -f "$output"

# KDE Store / Get New Widgets expects the zip root to contain metadata.json
# and contents/, not a nested package directory.
(
  cd "$package_dir"
  zip -r -9 "$output" . \
    -x "*.DS_Store" \
    -x "*~" \
    -x "*.swp" \
    -x "*/.git/*"
)

echo "Packaged $name $version"
echo "  id:       $plugin_id"
echo "  archive:  $output"
echo "  size:     $(du -h "$output" | awk '{print $1}')"
echo "  sha256:   $(sha256sum "$output" | awk '{print $1}')"

# Smoke-test: install the archive into a temporary package root.
test_root="$(mktemp -d "${TMPDIR:-/tmp}/codexbar-plasmoid-package.XXXXXX")"
cleanup() { rm -rf "$test_root"; }
trap cleanup EXIT

extract_dir="$test_root/extracted"
mkdir -p "$extract_dir"
unzip -q "$output" -d "$extract_dir"

if [[ ! -f "$extract_dir/metadata.json" || ! -d "$extract_dir/contents" ]]; then
  echo "Archive layout is invalid: expected metadata.json and contents/ at zip root." >&2
  exit 1
fi

kpackagetool6 --type Plasma/Applet --install "$extract_dir" --packageroot "$test_root/packages"
installed="$test_root/packages/$plugin_id"
if [[ ! -d "$installed" ]]; then
  echo "kpackagetool6 install failed for $plugin_id" >&2
  exit 1
fi

echo "Validated install under $installed"
