#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
package_dir="$repo_root/plasmoid"
old_ids=("org.kde.codexbar" "org.splazma.codexbar")
new_id="org.slopfire.codexbar-plasmoid"

if ! command -v kpackagetool6 >/dev/null 2>&1; then
  echo "kpackagetool6 is required to install the Plasma widget." >&2
  exit 1
fi

"$repo_root/scripts/build-native-cli.sh"

for old_id in "${old_ids[@]}"; do
  if kpackagetool6 --type Plasma/Applet --list | grep -q "$old_id"; then
    kpackagetool6 --type Plasma/Applet --remove "$old_id" >/dev/null || true
  fi
done

if kpackagetool6 --type Plasma/Applet --list | grep -q "$new_id"; then
  kpackagetool6 --type Plasma/Applet --upgrade "$package_dir"
else
  kpackagetool6 --type Plasma/Applet --install "$package_dir"
fi

echo "Installed $new_id"
