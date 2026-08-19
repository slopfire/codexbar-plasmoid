# store.kde.org authentication & upload

Scripts:

- `scripts/release-kde-store.sh` — package + upload (entry point)
- `scripts/lib/kde-store-auth.py` — Chrome session → cookie **file** only
- `scripts/lib/kde-store-browser-upload.py` — Files UI upload via isolated Chrome CDP

Product: `https://store.kde.org/p/2365275`

## Agent rule

**Do not decrypt or print cookies yourself.** Run the release script. If it asks for OAuth, tell the operator to finish the Chrome tab, then re-run the script. See `privacy.md`.

## Preferred order

```sh
./scripts/release-kde-store.sh
```

Auth discovery (inside the script, never printed):

1. `KDE_STORE_COOKIE_FILE` — Netscape cookie file **outside** the repo  
2. `KDE_STORE_COOKIE` — header string in the environment (copied into a temp file immediately)  
3. Local Chrome profile via `kde-store-auth.py write-cookie-file`

## Upload paths (in order)

### 1. curl product endpoints

1. `POST /p/2365275/addpploadfile/` multipart field `file_upload`  
2. `POST /p/2365275/updatepploadfile/` with `file_id`, `file_version`, `ocs_compatible=1`

### 2. Browser Files UI (automatic fallback)

`addpploadfile` often returns HTTP 200 with `{"status":"error","error_text":""}` even when the session is valid. The site’s **Files** dropzone still works.

Fallback (`kde-store-browser-upload.py`):

1. Isolated Chrome (`--user-data-dir` under `$TMPDIR`, remote debugging)  
2. Inject cookies from the temp cookie file via CDP (`Network.setCookie`) — **not** logged  
3. If edit page is login: OpenDesktop → GitHub OAuth; click Continue / Authorize (needs a signed-in GitHub session in Chrome)  
4. Edit product → **Files** (`#add-product-form-h-2`)  
5. Accept Terms → `DOM.setFileInputFiles` on `input[data-file-upload]` → **Add File(s)**  
6. Wait until the row has `data-ppload-file-id` **and** a 32-char MD5  
7. `updatepploadfile` with version + `ocs_compatible=1`  
8. Basics → set `#version` → **Save**  
9. Destroy CDP Chrome + user-data-dir  

Stdout is status-only, e.g. `upload:complete file_id=… md5=…` (artifact md5 is public).

### 3. Manual operator fallback

If automation still fails:

1. Open `https://store.kde.org/p/2365275/edit/` while signed in  
2. **Files** → accept terms → upload `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid` once  
3. Set file version to `X.Y.Z` (and leave OCS compatible on)  
4. **Basics** → product Version `X.Y.Z` → **Save**  
5. Delete accidental duplicates from failed retries  

## Session checks

```sh
# Status only: prints "authorized" or "unauthorized" (exit 1 if unauthorized)
uv run --quiet --with pycryptodome --with secretstorage --with browser-cookie3 \
  python3 scripts/lib/kde-store-auth.py check-edit
```

Success signals after release:

- Script ends with `Released CodexBar X.Y.Z to https://store.kde.org/p/2365275`  
- OCS lists the file (soft check):  
  `https://api.opendesktop.org/ocs/v1/content/data/2365275`  
  → `downloadnameN=codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`  
- Backend Basics version is `X.Y.Z` (OCS top-level `<version>` can lag)

## Failure modes → actions

| Symptom | Likely cause | Action |
|---------|--------------|--------|
| `no_chrome_store_session` | Not signed in / cookie decrypt failed | Sign into store in Chrome; retry |
| `unauthorized` / HTTP 302 on edit | Expired session | Open edit URL, finish GitHub OAuth, retry |
| curl `addpploadfile` → `status=error` empty text | Endpoint flaky vs Files UI | Script auto-falls back to browser upload |
| `auth_failed_sign_in_chrome` | OAuth not completed | Operator finishes OAuth in the tab; re-run |
| OCS missing new file briefly | Index lag | Wait; confirm on edit Files tab by MD5 |
| Duplicate `…-vX.Y.Z-…` rows | Retried uploads | Delete extras in Files UI |

## Hygiene

```sh
unset KDE_STORE_COOKIE KDE_STORE_COOKIE_FILE
rm -f "${TMPDIR:-/tmp}"/codexbar-kde-store*.cookies \
      "${TMPDIR:-/tmp}"/codexbar-chrome-cookies.* 2>/dev/null || true
rm -rf "${TMPDIR:-/tmp}"/codexbar-kde-cdp.* 2>/dev/null || true
```

The release script trap removes its own cookie file; still clean up if a run was killed mid-flight.
