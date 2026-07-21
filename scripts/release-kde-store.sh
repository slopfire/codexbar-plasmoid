#!/usr/bin/env bash
# Package and upload a release to the existing store.kde.org product.
set -Eeuo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
product_id="${KDE_STORE_PRODUCT_ID:-2365275}"
store_base="${KDE_STORE_BASE_URL:-https://store.kde.org}"
bump=false

usage() {
  cat <<'EOF'
Usage: ./scripts/release-kde-store.sh [--bump]

Packages and uploads the current widget version to store.kde.org.

Authentication is discovered automatically from the signed-in Google Chrome
profile. KDE_STORE_COOKIE_FILE or KDE_STORE_COOKIE can override discovery.

Options:
  --bump  Patch-bump and commit the version before packaging and uploading
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

for command in curl python3 uv; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required to release to store.kde.org." >&2
    exit 1
  fi
done

discover_chrome_cookie() {
  uv run --quiet --with browser-cookie3 python3 - "${HOME:?}" <<'PY'
import browser_cookie3
import pathlib
import sys

home = pathlib.Path(sys.argv[1])
user_data = home / ".config" / "google-chrome"
profiles = [user_data / "Default", *sorted(user_data.glob("Profile *"))]

for profile in profiles:
    database = profile / "Cookies"
    if not database.is_file():
        continue
    try:
        jar = browser_cookie3.chrome(cookie_file=str(database), domain_name="kde.org")
    except Exception:
        continue
    cookies = [cookie for cookie in jar if cookie.value]
    if any(cookie.name == "__ocs_id" for cookie in cookies):
        print("; ".join(f"{cookie.name}={cookie.value}" for cookie in cookies))
        raise SystemExit(0)

raise SystemExit(1)
PY
}

if [[ -n "${KDE_STORE_COOKIE_FILE:-}" ]]; then
  if [[ ! -f "$KDE_STORE_COOKIE_FILE" ]]; then
    echo "KDE_STORE_COOKIE_FILE does not exist: $KDE_STORE_COOKIE_FILE" >&2
    exit 1
  fi
  cookie_args=(-b "$KDE_STORE_COOKIE_FILE")
elif [[ -n "${KDE_STORE_COOKIE:-}" ]]; then
  cookie_args=(-b "$KDE_STORE_COOKIE")
elif discovered_cookie="$(discover_chrome_cookie)"; then
  cookie_args=(-b "$discovered_cookie")
  unset discovered_cookie
else
  echo "No signed-in store.kde.org session found in Google Chrome profiles." >&2
  echo "Sign in with Chrome and run this command again." >&2
  exit 1
fi

package_args=()
if [[ "$bump" == true ]]; then
  package_args+=(--bump)
fi
"$repo_root/scripts/package-plasmoid.sh" "${package_args[@]}"

readarray -t release_meta < <(python3 - "$repo_root/plasmoid/metadata.json" <<'PY'
import json
import sys

plugin = json.load(open(sys.argv[1]))["KPlugin"]
print(plugin["Id"].rsplit(".", 1)[-1])
print(plugin["Version"])
PY
)
short_name="${release_meta[0]}"
version="${release_meta[1]}"
archive="$repo_root/dist/${short_name}-v${version}-plasma6.plasmoid"

if [[ ! -f "$archive" ]]; then
  echo "Packaged archive not found: $archive" >&2
  exit 1
fi

upload_url="$store_base/p/$product_id/addpploadfile"
update_url="$store_base/p/$product_id/updatepploadfile"
upload_response="$(mktemp "${TMPDIR:-/tmp}/codexbar-kde-upload.XXXXXX")"
update_response="$(mktemp "${TMPDIR:-/tmp}/codexbar-kde-update.XXXXXX")"
cleanup() { rm -f "$upload_response" "$update_response"; }
trap cleanup EXIT

upload_archive() {
  curl -sS -o "$upload_response" -w '%{http_code}' \
    "${cookie_args[@]}" \
    -H 'Accept: application/json' \
    -H "Origin: $store_base" \
    -H "Referer: $store_base/p/$product_id/" \
    -A 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/150 Safari/537.36' \
    -F "file_upload=@$archive;type=application/zip" \
    "$upload_url/"
}

upload_status="$(upload_archive)"

if [[ "$upload_status" == 3* ]]; then
  chrome="$(command -v google-chrome || command -v google-chrome-stable || true)"
  if [[ -z "$chrome" ]]; then
    echo "Chrome is required to refresh the KDE Store session." >&2
    exit 1
  fi
  echo "Refreshing the KDE Store session through Chrome..."
  "$chrome" "$store_base/p/$product_id/edit/" >/dev/null 2>&1 &
  disown 2>/dev/null || true
  sleep 5
  if discovered_cookie="$(discover_chrome_cookie)"; then
    cookie_args=(-b "$discovered_cookie")
    unset discovered_cookie
    upload_status="$(upload_archive)"
  fi
  if [[ "$upload_status" == 3* ]]; then
    echo "Chrome did not refresh access to the protected product page." >&2
    echo "Open the tab Chrome created, complete any OAuth prompt, and retry." >&2
    exit 1
  fi
fi
if [[ "$upload_status" != 2* ]]; then
  echo "Store upload failed with HTTP $upload_status." >&2
  sed -n '1,12p' "$upload_response" >&2
  exit 1
fi

file_id="$(python3 - "$upload_response" <<'PY'
import json
import sys

try:
    response = json.load(open(sys.argv[1]))
except (OSError, json.JSONDecodeError) as error:
    raise SystemExit(f"Store returned a non-JSON upload response: {error}")
if response.get("status") != "ok" or not response.get("file", {}).get("id"):
    raise SystemExit(f"Store rejected upload: {response}")
print(response["file"]["id"])
PY
)"

update_status="$(curl -sS -o "$update_response" -w '%{http_code}' \
  "${cookie_args[@]}" \
  -H 'Accept: application/json' \
  -H "Origin: $store_base" \
  -H "Referer: $store_base/p/$product_id/" \
  -A 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/150 Safari/537.36' \
  --data-urlencode "file_id=$file_id" \
  --data-urlencode "file_version=$version" \
  --data-urlencode 'ocs_compatible=1' \
  "$update_url/")"

if [[ "$update_status" != 2* ]]; then
  echo "Archive uploaded as file $file_id, but version update failed with HTTP $update_status." >&2
  exit 1
fi

python3 - "$update_response" <<'PY'
import json
import sys

try:
    response = json.load(open(sys.argv[1]))
except (OSError, json.JSONDecodeError) as error:
    raise SystemExit(f"Store returned a non-JSON metadata response: {error}")
if response.get("status") != "ok":
    raise SystemExit(f"Archive uploaded, but store rejected its metadata: {response}")
PY

echo "Released CodexBar $version to $store_base/p/$product_id"
