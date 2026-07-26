# Validation Checklist

Use this checklist before saying the plasmoid work is complete.

## Preferred one-liner

```sh
./scripts/agent-check.sh
```

This runs shell `bash -n`, `qmllint`, `node --check`, mock helper smoke, virtual
`plasmawindowed`, and virtual form-factor viewers. Logs land under
`/tmp/codexbar-virtual-latest/`.

```sh
./scripts/agent-check.sh --quick     # static only
./scripts/agent-check.sh --install   # also install/upgrade the plasmoid
```

## Static Checks (manual)

```sh
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
bash -n scripts/*.sh
node --check plasmoid/contents/code/*.mjs
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/codexbar-plasma-package-test
```

Expected package id:

```text
org.slopfire.codexbar-plasmoid
```

## Virtual runtime (preferred — no host interruption)

```sh
./scripts/run-virtual-plasma.sh --timeout 30
cat /tmp/codexbar-virtual-latest/app.log   # no QML errors
```

Form factors (after `./scripts/install-plasmoid.sh`):

```sh
./scripts/run-virtual-plasma.sh --viewer planar --timeout 20
./scripts/run-virtual-plasma.sh --viewer horizontal --timeout 20
```

Mock CLI for helper/UI data:

```sh
./scripts/setup-mock-cli.sh
PATH=/tmp/codexbar-plasma-mock:$PATH ./scripts/run-virtual-plasma.sh --timeout 20
```

## Host windowed runtime (a11y / computer-use)

Only when you need AT-SPI interaction on the real desktop (opens a host window):

```sh
PATH=… plasmawindowed /home/sfire/Projects/slopfire/codexbar-plasmoid/plasmoid
```

With **computer-use-linux** (no screenshots unless the user asks):

1. Prefer virtual logs first; use host window only for interactive checks.
2. `list_windows` → activate `CodexBar` by `window_id`.
3. `get_app_state` with `include_screenshot: false`.
4. If only compact is visible, Press the Open/compact control, then re-read state.
5. Assert accessible text includes `CodexBar`, provider names, usage rows, percentages, credits/costs, and `Updated now` when data exists.
6. Configure should open **CodexBar Settings**.
7. Stop the viewer when done (`kill` the plasmawindowed PID or close the window).

## Common Runtime Failures

- `IDs cannot start with an uppercase letter`: QML object ids must start lowercase.
- `Cannot assign to non-existent property fullRepresentation`: root should be `PlasmoidItem`; assign `fullRepresentation` directly.
- `Cannot assign to non-existent property preferredRepresentation`: assign directly on `PlasmoidItem`, not as `Plasmoid.preferredRepresentation`.
- Blank metric labels with `Invalid arguments`: QML `Number.toLocaleString` does not accept browser options objects.
- `package plasmoid does not exist`: use absolute package path or `scripts/run-windowed.sh` / `scripts/run-virtual-plasma.sh`.
- `plasmoidviewer` with a filesystem path: use `-a org.slopfire.codexbar-plasmoid` after install instead.
