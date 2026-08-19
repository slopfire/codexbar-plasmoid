#!/usr/bin/env bash
# Package and upload a release to the existing store.kde.org product.
#
# Privacy: cookie values never go to stdout/stderr or the agent transcript.
# They are written only to a mode-0600 temp file under $TMPDIR and deleted on exit.
set -Eeuo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
product_id="${KDE_STORE_PRODUCT_ID:-2365275}"
store_base="${KDE_STORE_BASE_URL:-https://store.kde.org}"
auth_py="$repo_root/scripts/lib/kde-store-auth.py"
browser_upload_py="$repo_root/scripts/lib/kde-store-browser-upload.py"
bump=false

usage() {
  cat <<'EOF'
Usage: ./scripts/release-kde-store.sh [--bump]

Packages and uploads the current widget version to store.kde.org.

Authentication (never printed):
  1. KDE_STORE_COOKIE_FILE  — Netscape cookie file outside the repo
  2. KDE_STORE_COOKIE       — header string in the environment (written to a temp file)
  3. Local Chrome profile via scripts/lib/kde-store-auth.py

Upload order:
  1. POST /addpploadfile/ + /updatepploadfile/ (curl)
  2. On empty JSON error or failure: browser Files UI via CDP
     (scripts/lib/kde-store-browser-upload.py)

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

cookie_file=""
owned_cookie_file=false
upload_response=""
update_response=""

cleanup() {
  # Wipe auth material first; never cat these files.
  if [[ "$owned_cookie_file" == true && -n "$cookie_file" ]]; then
    rm -f -- "$cookie_file" 2>/dev/null || true
  fi
  rm -f -- ${upload_response:+"$upload_response"} ${update_response:+"$update_response"} 2>/dev/null || true
}
trap cleanup EXIT

write_env_cookie_to_file() {
  # Convert a Cookie header string into a minimal Netscape file without echoing it.
  local dest="$1"
  local header="${KDE_STORE_COOKIE-}"
  python3 - "$dest" <<'PY'
import pathlib
import sys

dest = pathlib.Path(sys.argv[1])
# Read from env inside Python so bash does not expand secrets into set -x traces.
import os
header = os.environ.get("KDE_STORE_COOKIE") or ""
if not header.strip():
    raise SystemExit(1)
lines = ["# Netscape HTTP Cookie File", "# from KDE_STORE_COOKIE", ""]
exp = 2000000000
for part in header.split(";"):
    part = part.strip()
    if not part or "=" not in part:
        continue
    name, value = part.split("=", 1)
    name = name.strip()
    value = value.strip().replace("\t", "").replace("\n", "").replace("\r", "")
    # Host-only on store.kde.org; curl still sends it for that host.
    lines.append(f"store.kde.org\tFALSE\t/\tTRUE\t{exp}\t{name}\t{value}")
dest.write_text("\n".join(lines) + "\n", encoding="utf-8")
dest.chmod(0o600)
print("ok")
PY
}

prepare_cookie_file() {
  cookie_file="$(mktemp "${TMPDIR:-/tmp}/codexbar-kde-store.XXXXXX.cookies")"
  owned_cookie_file=true
  chmod 600 "$cookie_file"

  if [[ -n "${KDE_STORE_COOKIE_FILE:-}" ]]; then
    if [[ ! -f "$KDE_STORE_COOKIE_FILE" ]]; then
      echo "KDE_STORE_COOKIE_FILE does not exist." >&2
      exit 1
    fi
    # Copy so cleanup does not delete the operator's file.
    cp -- "$KDE_STORE_COOKIE_FILE" "$cookie_file"
    chmod 600 "$cookie_file"
    echo "auth: cookie-file"
    return 0
  fi

  if [[ -n "${KDE_STORE_COOKIE:-}" ]]; then
    if ! write_env_cookie_to_file "$cookie_file" >/dev/null; then
      echo "KDE_STORE_COOKIE is set but empty or unusable." >&2
      exit 1
    fi
    echo "auth: env-cookie"
    return 0
  fi

  # Discover from Chrome — status only on stdout ("ok"), cookies only in file.
  if ! uv run --quiet --with pycryptodome --with secretstorage --with browser-cookie3 \
      python3 "$auth_py" write-cookie-file "$cookie_file" >/dev/null; then
    echo "No signed-in store.kde.org session found in Google Chrome profiles." >&2
    echo "Sign in at $store_base/p/$product_id/edit/ in Chrome, then retry." >&2
    exit 1
  fi
  echo "auth: chrome-profile"
}

session_authorized() {
  uv run --quiet --with pycryptodome --with secretstorage --with browser-cookie3 \
    python3 "$auth_py" check-edit "$cookie_file" >/dev/null 2>&1
}

refresh_session_via_chrome() {
  local chrome
  chrome="$(command -v google-chrome || command -v google-chrome-stable || true)"
  if [[ -z "$chrome" ]]; then
    echo "Chrome is required to refresh the KDE Store session." >&2
    return 1
  fi
  echo "Refreshing store session: complete any OAuth prompt in the Chrome tab, then this script continues..."
  "$chrome" "$store_base/p/$product_id/edit/" >/dev/null 2>&1 &
  disown 2>/dev/null || true
  # Re-discover after a short wait (operator may need longer; browser fallback also OAuths).
  sleep 8
  if ! uv run --quiet --with pycryptodome --with secretstorage --with browser-cookie3 \
      python3 "$auth_py" write-cookie-file "$cookie_file" >/dev/null; then
    return 1
  fi
  session_authorized
}

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

prepare_cookie_file

if ! session_authorized; then
  if ! refresh_session_via_chrome; then
    echo "Store session is not authorized for the product edit page." >&2
    echo "Open $store_base/p/$product_id/edit/ in Chrome, finish OAuth, then re-run." >&2
    exit 1
  fi
fi

upload_url="$store_base/p/$product_id/addpploadfile"
update_url="$store_base/p/$product_id/updatepploadfile"
upload_response="$(mktemp "${TMPDIR:-/tmp}/codexbar-kde-upload.XXXXXX")"
update_response="$(mktemp "${TMPDIR:-/tmp}/codexbar-kde-update.XXXXXX")"

upload_archive_curl() {
  curl -sS -o "$upload_response" -w '%{http_code}' \
    -b "$cookie_file" \
    -H 'Accept: application/json' \
    -H "Origin: $store_base" \
    -H "Referer: $store_base/p/$product_id/" \
    -A 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/150 Safari/537.36' \
    -F "file_upload=@$archive;type=application/zip" \
    "$upload_url/"
}

curl_upload_ok=false
file_id=""

upload_status="$(upload_archive_curl)"
if [[ "$upload_status" == 3* ]]; then
  if refresh_session_via_chrome; then
    upload_status="$(upload_archive_curl)"
  fi
fi

if [[ "$upload_status" == 2* ]]; then
  if file_id="$(python3 - "$upload_response" <<'PY'
import json
import sys

try:
    response = json.load(open(sys.argv[1]))
except (OSError, json.JSONDecodeError):
    raise SystemExit(1)
if response.get("status") != "ok" or not response.get("file", {}).get("id"):
    # Generic {"status":"error","error_text":""} is common; fall through to browser UI.
    raise SystemExit(1)
print(response["file"]["id"])
PY
)"; then
    curl_upload_ok=true
  else
    echo "curl addpploadfile returned HTTP $upload_status but rejected the payload; trying browser Files UI..."
  fi
else
  echo "curl addpploadfile failed with HTTP $upload_status; trying browser Files UI..."
fi

if [[ "$curl_upload_ok" == true ]]; then
  update_status="$(curl -sS -o "$update_response" -w '%{http_code}' \
    -b "$cookie_file" \
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
    raise SystemExit("Archive uploaded, but store rejected its metadata.")
PY
else
  # Browser Files UI path — cookies stay in $cookie_file; script prints status only.
  if ! uv run --quiet --with websockets --with pycryptodome --with secretstorage \
      python3 "$browser_upload_py" "$archive" "$version" "$cookie_file" "$product_id" "$store_base"; then
    echo "Browser Files UI upload failed." >&2
    echo "Manual fallback: open $store_base/p/$product_id/edit/ → Files → accept terms → upload $archive → set file version $version → set Basics version $version → Save." >&2
    exit 1
  fi
fi

# Public verification (no auth secrets): OCS API should list the new filename.
if python3 - "$version" "$short_name" <<'PY'
import sys
import urllib.request
import xml.etree.ElementTree as ET

version, short_name = sys.argv[1], sys.argv[2]
want = f"{short_name}-v{version}-plasma6.plasmoid"
url = "https://api.opendesktop.org/ocs/v1/content/data/2365275"
req = urllib.request.Request(url, headers={"User-Agent": "codexbar-plasmoid-release"})
try:
    xml = urllib.request.urlopen(req, timeout=30).read()
except Exception as exc:
    print(f"verify: ocs_fetch_failed ({type(exc).__name__})", file=sys.stderr)
    raise SystemExit(0)  # soft-fail verification
root = ET.fromstring(xml)
content = root.find(".//content")
names = []
for i in range(1, 12):
    name = content.findtext(f"downloadname{i}")
    if name:
        names.append(name)
if want not in names:
    print(f"verify: warning — OCS does not yet list {want}", file=sys.stderr)
    print(f"verify: listed={','.join(names) or '(none)'}", file=sys.stderr)
    raise SystemExit(0)
print(f"verify: ocs lists {want}")
PY
then
  :
fi

echo "Released CodexBar $version to $store_base/p/$product_id"
