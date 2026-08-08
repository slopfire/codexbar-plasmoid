# store.kde.org authentication

Upload script: `scripts/release-kde-store.sh`  
Product: `https://store.kde.org/p/2365275` (Plasma 6 applets)

## Preferred order

1. **Already signed-in Chrome** on this machine, then:

   ```sh
   ./scripts/release-kde-store.sh
   ```

   The script discovers cookies from local Chrome profiles without writing them into the repo.

2. **Operator-provided env** (local shell only, never commit):

   ```sh
   # Header style (name=value; name2=value2) — do not print this line in logs
   export KDE_STORE_COOKIE='…'
   ./scripts/release-kde-store.sh
   ```

   or a cookie file **outside** the repository:

   ```sh
   export KDE_STORE_COOKIE_FILE=/path/outside/repo/store.cookies
   ./scripts/release-kde-store.sh
   unset KDE_STORE_COOKIE_FILE
   ```

3. **Interactive refresh**: open the product edit URL in Chrome, complete OpenDesktop/GitHub OAuth if prompted, retry the script.

## Success signals

- `GET /p/2365275/edit/` stays on the edit UI (“welcome to your store backend”), not `/login` or opendesktop login.
- Upload script prints `Released CodexBar <version> to https://store.kde.org/p/2365275`.
- Public product page lists `codexbar-plasmoid-v<version>-plasma6.plasmoid` and shows version `<version>`.

## Failure modes → actions

| Symptom | Likely cause | Action |
|---------|--------------|--------|
| Script: no signed-in session found | Chrome locked cookies / logged out / decrypt failure | Sign into store in Chrome; retry. Script also tries keyring-backed decrypt fallback. |
| HTTP 302 on upload or edit | Session expired | Open edit URL in Chrome, finish OAuth, retry |
| HTTP 2xx but `{"status":"error"}` on `addpploadfile` | Endpoint expects browser/pling upload flow or stale CSRF/session | Use browser UI Files step: accept terms → add `.plasmoid` → set file version → save product version |
| Cookie decrypt errors from tooling | Wrong Safe Storage key or CBC payload prefix | Rely on the script fallback; do not copy keyring secrets into the repo |
| Duplicate versioned files on the listing | Multiple uploads during retries | Delete extras in the store Files UI; keep one clean `…-vX.Y.Z-plasma6.plasmoid` |

## Browser UI fallback (no secret capture)

When the script cannot upload but the operator can use a browser:

1. Open `https://store.kde.org/p/2365275/edit/` while signed in.
2. Go to **Files**.
3. Accept terms if required.
4. Upload `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid` only once.
5. Set the file version / OCS compatible flag to `X.Y.Z`.
6. Set product **Version** field to `X.Y.Z` and save Basics.
7. Confirm the public page listing.
8. Remove accidental duplicate uploads from failed retries.

If using automated browser control:

- Prefer an isolated `--user-data-dir` under `/tmp`, not the operator’s daily profile directory inside the repo.
- Inject sessions only in memory / CDP for that process.
- Delete the user-data-dir when finished.
- Never write cookie JSON into `.agents/` or project files.

## Hygiene after auth debugging

```sh
unset KDE_STORE_COOKIE KDE_STORE_COOKIE_FILE
rm -f /tmp/*kde*cookie* /tmp/*ocs*header* /tmp/chrome-session-cookies.json 2>/dev/null || true
rm -rf /tmp/chrome-kde-release 2>/dev/null || true
```

Do not describe cookie contents in the handoff note—only whether auth succeeded.
