---
name: kde-plasmoid-workflow
description: Use when developing, debugging, installing, renaming, or visually verifying the CodexBar KDE Plasma widget in this repository. Applies to files under plasmoid/, scripts/install-plasmoid.sh, scripts/run-windowed.sh, scripts/run-virtual-plasma.sh, Plasma/Kirigami QML, kpackagetool6, plasmawindowed, and isolated virtual KWin verification.
---

# KDE Plasmoid Workflow

## Scope

Use this skill for the repo at `/home/sfire/Projects/slopfire/codexbar-plasmoid` when work touches the Plasma package:

- `plasmoid/metadata.json`
- `plasmoid/contents/ui/*.qml`
- `plasmoid/contents/config/*`
- `scripts/install-plasmoid.sh`
- `scripts/run-windowed.sh`
- `scripts/run-virtual-plasma.sh`
- `scripts/run-nested-viewer.sh`
- README instructions for installing or running the widget

The current package ID is **`org.slopfire.codexbar-plasmoid`**.

Do not reintroduce `org.kde.codexbar` or `org.splazma.codexbar` except as migration/removal old IDs in installer code (`scripts/install-plasmoid.sh` already removes them).

## Package Shape

Expected Plasma 6 structure:

```text
plasmoid/
  metadata.json
  contents/
    code/codexbar-plasmoid-helper.mjs
    config/config.qml
    config/main.xml
    ui/main.qml
    ui/configGeneral.qml
    ui/*.qml
```

Important conventions:

- `metadata.json` must include `"KPackageStructure": "Plasma/Applet"` and `"X-Plasma-API-Minimum-Version": "6.0"`.
- `KPlugin.Id` must stay `org.slopfire.codexbar-plasmoid`.
- Root QML uses `PlasmoidItem`, not plain `Item`.
- In this Plasma install, assign `compactRepresentation`, `fullRepresentation`, `preferredRepresentation`, `toolTipMainText`, and `toolTipSubText` directly on `PlasmoidItem`.
- Use `preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation` so planar/desktop prefers the dashboard while panels use compact mode.
- Configuration entrypoint is `contents/config/config.qml`; config pages live under `contents/ui/config*.qml`.

## Workflow

1. Inspect current files before editing:

```sh
rg "org\\.slopfire\\.codexbar-plasmoid|org\\.splazma\\.codexbar|org\\.kde\\.codexbar" .
find plasmoid -type f | sort
```

2. Make scoped QML/package/script edits.

3. Run validation (preferred one-liner):

```sh
./scripts/agent-check.sh
# or static only:
./scripts/agent-check.sh --quick
```

Manual equivalent:

```sh
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
bash -n scripts/*.sh
node --check plasmoid/contents/code/*.mjs
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/codexbar-plasma-package-test
```

4. **Runtime UI (preferred — does not interrupt host desktop):**

```sh
./scripts/run-virtual-plasma.sh --timeout 30
# Stable log pointers:
#   /tmp/codexbar-virtual-latest/app.log
#   /tmp/codexbar-virtual-latest/session.log
```

Longer interactive virtual session:

```sh
./scripts/run-virtual-plasma.sh --keep
# … work …
./scripts/run-virtual-plasma.sh --stop wayland-codexbar-<id>
```

Form factors (install first):

```sh
./scripts/install-plasmoid.sh
./scripts/run-virtual-plasma.sh --viewer planar --timeout 20
./scripts/run-virtual-plasma.sh --viewer horizontal --timeout 20
```

5. **Host-visible** windowed run only when a11y/computer-use interaction is needed (opens a real window):

```sh
./scripts/run-windowed.sh
# or:
plasmawindowed /home/sfire/Projects/slopfire/codexbar-plasmoid/plasmoid
```

Use skill **computer-use-linux** (no screenshots unless asked). Expand compact → full before asserting dashboard labels.

6. After implementing a feature, install with `./scripts/install-plasmoid.sh` (see `AGENTS.md`).

## Runtime Notes

| Command | Notes |
|---------|--------|
| `./scripts/run-virtual-plasma.sh` | Isolated `dbus-run-session` + `kwin_wayland --virtual`. **No host window.** Best default for agents. |
| `./scripts/run-nested-viewer.sh` | Nested windowed KWin — one host window. |
| `./scripts/run-windowed.sh` | Builds native CLI then `plasmawindowed` on the **host**. |
| `plasmawindowed ./plasmoid` | May fail (`package plasmoid does not exist`); use **absolute** path. |
| `plasmoidviewer /path/to/package` | **Wrong** on Plasma 6 — path is not a package arg. Use `-a org.slopfire.codexbar-plasmoid` after install. |

Known `plasmawindowed` behavior:

- Configure **does open** **CodexBar Settings** (tabs: General, Providers, Appearance, Tray, Keyboard Shortcuts, About). Validate with AT-SPI or visual check.
- May start showing **compact** representation; expand (Open) to see full dashboard even when preferredRepresentation is full for planar in other hosts.
- `kpackagetool6 --list` can print unrelated third-party warnings. Success is install/upgrade of `org.slopfire.codexbar-plasmoid`.

## Visual / a11y requirements

The full representation should show, when data is present:

- heading plus refresh/configure icon buttons
- provider switch chips when more than one provider exists
- provider card title, account/source/version subtitle, and status badge
- Session/Weekly/Opus or provider rows with percent remaining bars
- Code review, credits, Today cost/tokens, 30d cost/tokens when present
- recent history chart when daily usage exists
- updated timestamp

The compact representation should show provider name plus the selected compact metric (e.g. `Codex · — · now`).

## References

Read `references/validation.md` when doing final checks.
