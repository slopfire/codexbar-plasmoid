# Release privacy rules

This project is published publicly on GitHub. Agents must optimize for **zero secret leakage** — including **into the agent transcript**.

## Secrets inventory (local only)

These may exist on the operator machine during publish. They are **not** project source:

- Browser cookies for `store.kde.org` / `*.kde.org` / `*.opendesktop.org` / `github.com` (OAuth)
- Chrome OSCrypt keyring entries (Safe Storage passwords)
- `KDE_STORE_COOKIE` / `KDE_STORE_COOKIE_FILE` shell values
- Temporary cookie files under `$TMPDIR` (mode 0600)
- OAuth redirect `code` query parameters

## Cookie isolation (agent-facing)

**Cookies must never enter the agent context.** That means:

| Do | Do not |
|----|--------|
| Run `./scripts/release-kde-store.sh` and read only its status lines | Write one-off Python/shell that decrypts Chrome cookies and prints them |
| Let `scripts/lib/kde-store-auth.py` write a **temp file** (mode 0600) | Capture cookie headers into variables that appear in tool logs / command strings the model sees |
| Pass `-b "$cookie_file"` **inside** scripts | `echo "$KDE_STORE_COOKIE"`, `print(cookie)`, or include cookie values in `curl -v` output to the transcript |
| Report `auth: chrome-profile` / `authorized` / `unauthorized` | Dump Netscape cookie files, `__ocs_id` values, or `remember_token` |

If you need to debug auth, report **only** high-level outcomes:

- `no_chrome_store_session`
- `unauthorized` (edit page redirected to login)
- `auth_failed_sign_in_chrome` (operator must finish OAuth in the opened tab)

Never paste cookie files, keyring secrets, or OAuth codes into chat, skills, commits, or PR text.

## Allowed local handling (scripts only)

- Read Chrome cookies **inside** `scripts/lib/kde-store-auth.py` / release helpers.
- Write cookies only to a short-lived temp file (mode 0600), not into the git worktree.
- Delete temp auth artifacts after success or failure (`release-kde-store.sh` trap).
- Isolated Chrome `--user-data-dir` under `$TMPDIR` for CDP upload; remove afterward.

## Forbidden

- Adding cookie files to the repo (even gitignored dumps)
- Putting secrets in `.agents/`, `README.md`, `CHANGELOG.md`, issues, or PR text
- Logging full cookie headers in script output intended for transcripts
- Committing screenshots that show session tokens or account recovery codes
- Storing store passwords in skills “for convenience”
- Copying the operator’s main Chrome profile into the repository
- Agent-authored `curl -b 'name=secret…'` command lines (secrets land in session logs)

## What you may cite publicly

- Store product URL and numeric product id `2365275`
- Version numbers, archive filenames, **md5/sha256 of the release artifact**
- Public `file_id` integers from a successful upload status line
- High-level failure modes: `302 to OAuth login`, `addpploadfile status=error`, `ocs lag`
- Public git commit hashes after push

## Redaction checklist before finishing a release turn

- [ ] `git status` shows no secret paths
- [ ] No cookie values in the final assistant message **or** intermediate tool outputs you re-quote
- [ ] Temp auth / CDP profile dirs removed
- [ ] Only intended release commits pushed
