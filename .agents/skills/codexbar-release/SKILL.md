---
name: codexbar-release
description: Package and publish the CodexBar Plasma widget (version bump, changelog, .plasmoid archive, store.kde.org upload, git push). Use when the user asks to release, publish, bump version, upload to KDE Store, or ship a new plasmoid package.
---

# CodexBar Release & Publish

## Scope

Use this skill only for **shipping** a release of this repository:

- version bump / changelog
- `dist/*.plasmoid` packaging
- store.kde.org upload
- pushing the release commit

Do **not** use it for routine widget development (that is `kde-plasmoid-workflow`).

## Public-repo privacy (non-negotiable)

This repository is **public**. Treat every file you write here as world-readable.

**Cookies and session tokens must never enter the agent transcript** (tool args, stdout you re-quote, chat, commits). Run only the release scripts; they keep auth in mode-0600 temp files and print status words only.

| Never | OK |
|-------|-----|
| Cookie headers / values / Netscape dumps | Product id `2365275` |
| `__ocs_id`, `remember_token`, OAuth codes | Script names and flags |
| Keyring / Safe Storage secrets | Version, archive **md5/sha256** |
| One-off decrypt scripts that `print` cookies | “Session authorized” / “OAuth needed” |

Hard rules:

1. **Never** commit `dist/`, cookie files, `/tmp/*cookie*`, or browser profile copies.
2. **Never** decrypt Chrome cookies in ad-hoc agent commands — use `scripts/lib/kde-store-auth.py` only (file out, status on stdout).
3. Prefer `./scripts/release-kde-store.sh` over hand-rolled `curl` with `-b`.
4. Delete temp auth artifacts when done (script trap + hygiene in `references/privacy.md`).
5. When stuck on auth, ask the operator to sign into `store.kde.org` in Chrome — do not scrape password managers.

Read `references/privacy.md` before any store work. Upload details: `references/store-auth.md`.

## Release facts

| | |
|--|--|
| Plugin id | `org.slopfire.codexbar-plasmoid` |
| Package dir | `plasmoid/` |
| Archive name | `codexbar-plasmoid-v<version>-plasma6.plasmoid` |
| Output dir | `dist/` (gitignored) |
| Store product | `https://store.kde.org/p/2365275` |
| Version sources (must match) | `plasmoid/metadata.json`, `native-cli/Cargo.toml`, `native-cli/Cargo.lock` (`codexbar-plasmoid` package only), README install examples, `CHANGELOG.md` |
| Release commit files | those five paths only |
| Native helper | built by packaging into gitignored `plasmoid/contents/code/codexbar-plasmoid` |
| Upload entrypoint | `./scripts/release-kde-store.sh` |

## Preconditions

```sh
git status -sb          # clean tree before --bump; for manual bump keep only intentional release edits
git log -5 --oneline
./scripts/agent-check.sh --quick   # or full ./scripts/agent-check.sh if UI/helper changed
```

Required tools: `git`, `python3`, `zip`, `cargo`, `kpackagetool6`, `curl`, `uv`, Chrome (store session / CDP fallback).

## Happy path (feature release)

Prefer a **real changelog**, then package, upload, push.

### 1. Collect user-facing notes

```sh
git log --oneline --grep='chore(release)' -1
git log <last-release-commit>..HEAD --oneline --no-merges
```

Write `CHANGELOG.md` entry at the top (after the intro paragraph):

```markdown
## X.Y.Z — YYYY-MM-DD

### Features
- …

### Fixes
- …

### Maintenance
- …
```

Omit empty sections. Do not invent changes.

### 2. Synchronize version `X.Y.Z`

Update **all** of:

- `plasmoid/metadata.json` → `KPlugin.Version`
- `native-cli/Cargo.toml` → package `version`
- `native-cli/Cargo.lock` → only the `name = "codexbar-plasmoid"` package stanza (leave unrelated crates alone)
- `README.md` install/upgrade archive filenames
- `CHANGELOG.md` heading for `X.Y.Z`

### 3. Package + validate

```sh
./scripts/package-plasmoid.sh
```

Expect: native helper rebuild, `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`, size + sha256, temp `kpackagetool6 --install` success.

### 4. Commit release metadata

```sh
git add plasmoid/metadata.json native-cli/Cargo.toml native-cli/Cargo.lock README.md CHANGELOG.md
git commit --no-gpg-sign -m "chore(release): bump version to X.Y.Z"
```

Do not stage the native binary (gitignored). Feature/fix commits land **before** this bump commit.

### 5. Publish to KDE Store

```sh
./scripts/release-kde-store.sh
```

What the script does (agents do not reimplement this):

1. Auth → temp cookie file only (`kde-store-auth.py` or env/file override)  
2. Checks edit-page authorization (status only)  
3. Tries `curl` `addpploadfile` + `updatepploadfile`  
4. On empty JSON error / failure → **browser Files UI** via CDP (`kde-store-browser-upload.py`)  
5. Soft-verifies OCS lists `codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`  
6. Prints `Released CodexBar X.Y.Z to https://store.kde.org/p/2365275`  
7. Deletes the cookie file on exit  

If the script says OAuth is required: ask the operator to finish the Chrome tab, then **re-run the same script**. Do not start decrypting cookies in the agent shell.

### 6. Push

```sh
git push origin HEAD
```

Default branch is `master`. No git tag unless the operator asks.

### 7. Verify

- Archive at `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`  
- Metadata version = changelog = release commit  
- OCS download entry for that filename (top-level OCS `<version>` may lag)  
- `git status` clean and not ahead of origin  

## Maintenance-only path (`--bump`)

Clean tree only:

```sh
./scripts/release-kde-store.sh --bump
git push origin HEAD
```

Or package bump without upload: `./scripts/package-plasmoid.sh --bump` then `./scripts/release-kde-store.sh`.

## Changelog quality bar

- User-facing outcomes, not commit hashes or agent tooling trivia  
- Features / Fixes / Maintenance  
- Skip agent-only workflow noise unless package consumers care  

## Anti-patterns

- Bumping only `metadata.json`  
- Rewriting unrelated `Cargo.lock` stanzas that share the old patch version  
- Committing `dist/` or `*.plasmoid`  
- Restarting plasmashell or host `plasmawindowed` for release validation  
- **Decrypting store cookies in agent one-liners** or pasting them into tool calls  
- Creating GitHub Releases/tags unless asked  

## Related skills

| Skill | Role |
|-------|------|
| `kde-plasmoid-workflow` | Dev install / virtual Plasma validation before release |
| `kde-dev-tools` | kpackagetool6 / qmllint background |
| `codexbar-cli-bridge` | Helper/CLI contract if the release includes helper changes |
