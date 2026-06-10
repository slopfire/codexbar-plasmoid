# Validation Checklist

Use this checklist before saying the plasmoid work is complete.

## Static Checks

```sh
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
bash -n scripts/install-plasmoid.sh scripts/run-windowed.sh
node --check plasmoid/contents/code/codexbar-plasmoid-helper.mjs
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/codexbar-plasma-package-test
```

Expected package output path should end with:

```text
/org.splazma.codexbar/
```

## Windowed Runtime

Create/use a mock CLI on `PATH` before opening live provider code. Then run:

```sh
PATH=/tmp/codexbar-plasma-mock:$PATH plasmawindowed /home/sfire/slop/kde-codexbar/plasmoid
```

With KWin MCP:

1. Start a virtual session with `plasmawindowed /home/sfire/slop/kde-codexbar/plasmoid`.
2. Read the app log; there should be no QML load errors.
3. Capture a screenshot.
4. Use the accessibility tree to verify visible text includes `CodexBar`, provider names, usage rows, percentages, credits/costs, and `Updated now`.
5. Stop the session.

## Common Runtime Failures

- `IDs cannot start with an uppercase letter`: QML object ids must start lowercase.
- `Cannot assign to non-existent property fullRepresentation`: root should be `PlasmoidItem`; assign `fullRepresentation` directly.
- `Cannot assign to non-existent property preferredRepresentation`: assign directly on `PlasmoidItem`, not as `Plasmoid.preferredRepresentation`.
- Blank metric labels with `Invalid arguments`: QML `Number.toLocaleString` does not accept browser options objects.
- `package plasmoid does not exist`: use absolute package path or `scripts/run-windowed.sh`.
