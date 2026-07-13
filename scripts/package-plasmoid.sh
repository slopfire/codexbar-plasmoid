#!/usr/bin/env bash
# Build a store-ready .plasmoid archive (zip of package contents).
set -Eeuo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$repo_root/plasmoid"
metadata="$package_dir/metadata.json"
dist_dir="$repo_root/dist"
bump=false
bump_applied=false
release_files=(
  "plasmoid/metadata.json"
  "native-cli/Cargo.toml"
  "native-cli/Cargo.lock"
  "README.md"
  "CHANGELOG.md"
)

usage() {
  cat <<'EOF'
Usage: ./scripts/package-plasmoid.sh [--bump]

  --bump  Increment the patch version, package it, and commit the version files.
EOF
}

case "${1:-}" in
  "") ;;
  --bump) bump=true ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

if (( $# > 1 )); then
  usage >&2
  exit 2
fi

if ! command -v zip >/dev/null 2>&1; then
  echo "zip is required to package the Plasma widget." >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to read package metadata." >&2
  exit 1
fi

rollback_bump() {
  trap - ERR
  if [[ "$bump_applied" == true ]]; then
    git -C "$repo_root" restore --source=HEAD --staged --worktree -- "${release_files[@]}"
    echo "Packaging failed; restored version files." >&2
  fi
}

if [[ "$bump" == true ]]; then
  if ! command -v git >/dev/null 2>&1; then
    echo "git is required when using --bump." >&2
    exit 1
  fi
  if [[ -n "$(git -C "$repo_root" status --porcelain --untracked-files=normal)" ]]; then
    echo "--bump requires a clean Git working tree." >&2
    exit 1
  fi

  bump_output="$(python3 - "$repo_root" "$(date +%F)" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
release_date = sys.argv[2]
metadata_path = root / "plasmoid/metadata.json"
cargo_path = root / "native-cli/Cargo.toml"
lock_path = root / "native-cli/Cargo.lock"
readme_path = root / "README.md"
changelog_path = root / "CHANGELOG.md"

metadata_text = metadata_path.read_text()
current = json.loads(metadata_text)["KPlugin"]["Version"]
match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", current)
if not match:
    raise SystemExit(f"Cannot patch-bump non-semver version: {current}")
major, minor, patch = map(int, match.groups())
new = f"{major}.{minor}.{patch + 1}"

old_metadata = f'"Version": "{current}"'
if metadata_text.count(old_metadata) != 1:
    raise SystemExit("Could not locate exactly one Plasma metadata version")
metadata_text = metadata_text.replace(old_metadata, f'"Version": "{new}"')

cargo_text = cargo_path.read_text()
old_cargo = f'version = "{current}"'
if cargo_text.count(old_cargo) < 1:
    raise SystemExit("Cargo.toml version does not match Plasma metadata")
cargo_text = cargo_text.replace(old_cargo, f'version = "{new}"', 1)

lock_text = lock_path.read_text()
lock_pattern = re.compile(
    rf'(\[\[package\]\]\nname = "codexbar-plasmoid"\nversion = "){re.escape(current)}(")'
)
lock_text, lock_count = lock_pattern.subn(rf'\g<1>{new}\2', lock_text, count=1)
if lock_count != 1:
    raise SystemExit("Cargo.lock package version does not match Plasma metadata")

readme_text = readme_path.read_text()
old_archive = f"codexbar-plasmoid-v{current}-plasma6.plasmoid"
if old_archive not in readme_text:
    raise SystemExit("README archive examples do not match Plasma metadata")
readme_text = readme_text.replace(
    old_archive, f"codexbar-plasmoid-v{new}-plasma6.plasmoid"
)

changelog_text = changelog_path.read_text()
heading = f"## {new} — {release_date}"
if heading in changelog_text:
    raise SystemExit(f"CHANGELOG already contains {new}")
marker = "All notable changes to the CodexBar Plasma widget are documented in this file.\n\n"
if marker not in changelog_text:
    raise SystemExit("Could not locate CHANGELOG insertion point")
entry = f"{heading}\n\n### Maintenance\n\n- Bump release version\n\n"
changelog_text = changelog_text.replace(marker, marker + entry, 1)

metadata_path.write_text(metadata_text)
cargo_path.write_text(cargo_text)
lock_path.write_text(lock_text)
readme_path.write_text(readme_text)
changelog_path.write_text(changelog_text)
print(current)
print(new)
PY
  )"
  bump_applied=true
  trap rollback_bump ERR
  readarray -t bumped_versions <<<"$bump_output"
  if (( ${#bumped_versions[@]} != 2 )); then
    echo "Version bump returned unexpected output." >&2
    false
  fi
  previous_version="${bumped_versions[0]}"
  bumped_version="${bumped_versions[1]}"
  echo "Bumped version $previous_version -> $bumped_version"
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

if [[ "$bump" == true ]]; then
  git -C "$repo_root" commit --only --no-gpg-sign \
    -m "chore(release): bump version to $bumped_version" \
    -- "${release_files[@]}"
  bump_applied=false
  trap - ERR
  echo "Committed release version $bumped_version"
fi
