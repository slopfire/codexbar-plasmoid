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

| Never put in the repo, commits, PRs, skills, or logs | OK to record |
|-----------------------------------------------------|--------------|
| Cookie headers / `KDE_STORE_COOKIE` values | Product id `2365275` (public listing) |
| `__ocs_id`, `remember_token`, OAuth codes | Script names and flags |
| Passwords, API keys, personal emails beyond existing public metadata | Version numbers, sha256 of the archive |
| Decrypted Chrome cookie dumps, Netscape cookie files | High-level auth *method* names |
| Full `Authorization` headers or session JSON | “Signed in as store owner” without secrets |

Hard rules for agents:

1. **Never** commit under `dist/`, cookie files, `/tmp/*cookie*`, or browser profile copies.
2. **Never** paste cookie values, tokens, or passwords into chat, commit messages, or skill files.
3. Prefer env vars in the **local shell only**: `KDE_STORE_COOKIE`, `KDE_STORE_COOKIE_FILE`.
4. If a command would print secrets, redirect or redact; delete temp auth files when done.
5. Auth recovery may use the operator’s local Chrome keyring **on this machine only** — do not export keyring secrets into the tree.
6. When stuck on auth, ask the operator to sign into store.kde.org in Chrome rather than scraping credentials from password managers.

Read `references/privacy.md` before any store authentication work.

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

## Preconditions

```sh
git status -sb          # clean tree before --bump; for manual bump keep only intentional release edits
git log -5 --oneline
./scripts/agent-check.sh --quick   # or full ./scripts/agent-check.sh if UI/helper changed
```

Required tools: `git`, `python3`, `zip`, `cargo`, `kpackagetool6`, `curl`, `uv`, Chrome (for store session).

## Happy path (feature release)

Prefer a **real changelog**, then package, upload, push.

### 1. Collect user-facing notes

```sh
# since previous release commit
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
- `native-cli/Cargo.lock` → only the `name = "codexbar-plasmoid"` package stanza (leave unrelated crates alone, e.g. `inout 0.1.4`)
- `README.md` install/upgrade archive filenames
- `CHANGELOG.md` heading for `X.Y.Z`

### 3. Package + validate

```sh
./scripts/package-plasmoid.sh
```

Expect:

- native helper rebuild
- `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`
- printed **size** + **sha256**
- temp `kpackagetool6 --install` success

### 4. Commit release metadata

```sh
git add plasmoid/metadata.json native-cli/Cargo.toml native-cli/Cargo.lock README.md CHANGELOG.md
git commit --no-gpg-sign -m "chore(release): bump version to X.Y.Z"
```

Do not stage the native binary (gitignored).

### 5. Publish to KDE Store

Primary:

```sh
./scripts/release-kde-store.sh
```

This re-packages the **current** metadata version (no bump) and uploads.

Auth order inside the script:

1. `KDE_STORE_COOKIE_FILE` (Netscape or header cookie file outside the repo)
2. `KDE_STORE_COOKIE` (header string in the environment)
3. Local Chrome profile cookie discovery (see `references/store-auth.md`)

On success: `Released CodexBar X.Y.Z to https://store.kde.org/p/2365275`.

If auth fails, follow `references/store-auth.md`. Do **not** commit workaround dumps.

### 6. Push

```sh
git push origin HEAD
```

Default branch is `master`. No git tag is required by current process (releases are store + commit based).

### 7. Verify

- Archive still at `dist/codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`
- `plasmoid/metadata.json` version = changelog = commit message
- Store product page shows **version X.Y.Z** and a file named like  
  `codexbar-plasmoid-vX.Y.Z-plasma6.plasmoid`  
  (public page is JS-rendered; use a logged-in or headless browser if curl HTML lacks filenames)
- `git status` clean and not ahead of origin

## Maintenance-only path (`--bump`)

For empty “version bump only” releases from a **clean** tree:

```sh
./scripts/package-plasmoid.sh --bump
# optional: edit CHANGELOG.md if the auto "Maintenance / Bump release version" stub is too thin,
# then amend or make a tiny follow-up commit before upload
./scripts/release-kde-store.sh
git push origin HEAD
```

`--bump` patch-increments semver, syncs the five release files, packages, validates, and commits.  
It **fails** if the working tree is dirty or the changelog already contains the new heading.

Combined form (bump + upload in one shot):

```sh
./scripts/release-kde-store.sh --bump
git push origin HEAD
```

## Store upload behavior (implementation notes)

`scripts/release-kde-store.sh`:

1. Builds via `package-plasmoid.sh`
2. `POST` multipart to `/p/2365275/addpploadfile/` as `file_upload`
3. `POST` `/p/2365275/updatepploadfile/` with `file_id`, `file_version`, `ocs_compatible=1`

The website UI may instead upload through a pling.com file server first; the script uses the product endpoints. If those return generic `{"status":"error"}` while the browser UI works, use the authenticated browser fallback in `references/store-auth.md` without recording session material.

## Changelog quality bar

- User-facing outcomes, not commit hashes or agent tooling trivia
- Group under Features / Fixes / Maintenance
- Keep agent-only workflow changes out unless they affect package consumers

## Anti-patterns

- Bumping only `metadata.json` and forgetting Cargo/README/changelog
- Rewriting unrelated `Cargo.lock` versions that merely equal the old patch
- Committing `dist/` or `*.plasmoid`
- Restarting the operator’s plasmashell to “test the release”
- Opening host `plasmawindowed` without explicit approval (use `agent-check` / virtual plasma before release)
- Echoing `KDE_STORE_COOKIE` into CI, dotenv files inside the repo, or skill references
- Creating GitHub Releases/tags unless the operator asks (not part of the default loop today)

## Related skills

| Skill | Role |
|-------|------|
| `kde-plasmoid-workflow` | Dev install / virtual Plasma validation before release |
| `kde-dev-tools` | kpackagetool6 / qmllint background |
| `codexbar-cli-bridge` | Helper/CLI contract if the release includes helper changes |
