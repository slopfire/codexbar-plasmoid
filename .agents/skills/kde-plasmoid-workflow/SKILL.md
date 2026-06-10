---
name: kde-plasmoid-workflow
description: Use when developing, debugging, installing, renaming, or visually verifying the Splazma CodexBar KDE Plasma widget in this repository. Applies to files under plasmoid/, scripts/install-plasmoid.sh, scripts/run-windowed.sh, Plasma/Kirigami QML, kpackagetool6, plasmawindowed, and KWin MCP verification.
---

# KDE Plasmoid Workflow

## Scope

Use this skill for the repo at `/home/sfire/slop/kde-codexbar` when work touches the Plasma package:

- `plasmoid/metadata.json`
- `plasmoid/contents/ui/*.qml`
- `plasmoid/contents/config/*`
- `scripts/install-plasmoid.sh`
- `scripts/run-windowed.sh`
- README instructions for installing or running the widget

The current package ID is `org.splazma.codexbar`. Do not reintroduce `org.kde.codexbar` except as a migration/removal old ID in installer code.

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
- `KPlugin.Id` must stay `org.splazma.codexbar`.
- Root QML uses `PlasmoidItem`, not plain `Item`.
- In this Plasma install, assign `compactRepresentation`, `fullRepresentation`, `preferredRepresentation`, `toolTipMainText`, and `toolTipSubText` directly on `PlasmoidItem`.
- Use `preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation` so `plasmawindowed` shows the dashboard while panels use compact mode.
- Configuration entrypoint is `contents/config/config.qml`; config page QML can live under `contents/ui/configGeneral.qml`.

## Workflow

1. Inspect current files before editing:

```sh
rg "org\\.splazma\\.codexbar|org\\.kde\\.codexbar" .
find plasmoid -type f | sort
```

2. Make scoped QML/package/script edits.

3. Run validation:

```sh
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
bash -n scripts/install-plasmoid.sh scripts/run-windowed.sh
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/codexbar-plasma-package-test
```

4. For runtime UI changes, verify in KWin MCP with a mock `codexbar` on `PATH`. Use the `kwin-desktop-automation` skill when driving KWin MCP.

5. Stop KWin MCP sessions at the end.

## Runtime Notes

Windowed run:

```sh
./scripts/run-windowed.sh
plasmawindowed /home/sfire/slop/kde-codexbar/plasmoid
```

Installed run:

```sh
./scripts/install-plasmoid.sh
plasmawindowed org.splazma.codexbar
```

Known `plasmawindowed` behavior:

- `plasmawindowed ./plasmoid` may be interpreted as a component ID and fail with `package plasmoid does not exist`; use the absolute package path or `scripts/run-windowed.sh`.
- The configure button may not open a dialog in `plasmawindowed`; validate config package files with `kpackagetool6` and normal Plasma install behavior.
- `kpackagetool6 --list` can print unrelated warnings for third-party widgets whose `KPackageStructure` differs. Success is indicated by installing/upgrading `org.splazma.codexbar`.

## Visual Requirements

The full representation should show, when data is present:

- heading plus refresh/configure icon buttons
- provider switch chips when more than one provider exists
- provider card title, account/source/version subtitle, and status badge
- Session/Weekly/Opus or provider rows with percent remaining bars
- Code review, credits, Today cost/tokens, 30d cost/tokens when present
- recent history chart when daily usage exists
- updated timestamp

The compact representation should show provider name plus the selected compact metric.

## References

Read `references/validation.md` when doing final checks or KWin MCP verification.
