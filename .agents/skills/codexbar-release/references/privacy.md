# Release privacy rules

This project is published publicly on GitHub. Agents must optimize for **zero secret leakage**.

## Secrets inventory (local only)

These may exist on the operator machine during publish. They are **not** project source:

- Browser cookies for `store.kde.org` / `*.kde.org` / `*.opendesktop.org`
- Chrome OSCrypt keyring entries (Safe Storage passwords)
- `KDE_STORE_COOKIE` / `KDE_STORE_COOKIE_FILE` shell values
- Temporary files under `/tmp` created while debugging auth
- OAuth redirect `code` query parameters

## Allowed local handling

- Read cookies from the operator’s Chrome profile **in memory** or short-lived temp files.
- Pass cookies to `curl` via env/`-b` without writing them into the git worktree.
- Delete temp auth artifacts after a successful or abandoned publish:

```sh
# example patterns only — never commit these paths
rm -f /tmp/*kde*cookie* /tmp/*ocs* /tmp/chrome-session-cookies.json 2>/dev/null || true
```

- If browser automation is required, use an isolated Chrome user-data-dir under `/tmp` and remove it afterward.

## Forbidden

- Adding cookie files to the repo (even gitignored dumps that might be force-added later)
- Putting secrets in `.agents/`, `README.md`, `CHANGELOG.md`, issues, or PR text
- Logging full cookie headers in script output intended for transcripts
- Committing screenshots that show session tokens or account recovery codes
- Storing store passwords in skills “for convenience”
- Copying the operator’s main Chrome profile into the repository

## What you may cite publicly

- Store product URL and numeric product id
- Version numbers, archive filenames, sha256 of the **release artifact**
- High-level failure modes: “302 to OAuth login”, “cookie decrypt failed”, “upload JSON status=error”
- Public git commit hashes after push

## Redaction checklist before finishing a release turn

- [ ] `git status` shows no secret paths
- [ ] No cookie values in the final assistant message
- [ ] Temp auth files removed
- [ ] Isolated browser profiles removed
- [ ] Only release metadata commit pushed
