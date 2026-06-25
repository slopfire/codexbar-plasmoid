# CodexBar Plasmoid — Agent Rules

## Technology Stack

- **Plasma 6** widget (pure QML — no CMake, no C++ plugin)
- **Qt 6 QML** with ES module helpers (`.mjs`)
- Package ID: `org.slopfire.codexbar-plasmoid`
- Root element: `PlasmoidItem` (never plain `Item`)

## Import Conventions

Use Plasma 6 style — no version numbers except PlasmaComponents:

```qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid
```

## QML Rules

- Assign `compactRepresentation`, `fullRepresentation`, `preferredRepresentation`,
  `toolTipMainText`, `toolTipSubText` directly on `PlasmoidItem` — never as
  `Plasmoid.fullRepresentation`.
- QML object IDs must start lowercase (`id: myItem`, never `id: MyItem`).
- Use `Kirigami.Units` for spacing and sizing — never hardcode pixel values.
- Use `Kirigami.Theme` for colors — never hardcode color values.
- Number formatting: use `Number(x).toLocaleString(Qt.locale(), 'f', digits)` —
  never browser-style `toLocaleString(locale, options)`.
- Use `i18n()` for all user-visible strings.
- Never mix `anchors.fill` and `Layout.fillWidth` on the same item.

## Validation Checklist

Run before declaring any plasmoid work complete:

```bash
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
bash -n scripts/install-plasmoid.sh scripts/run-windowed.sh
node --check plasmoid/contents/code/codexbar-plasmoid-helper.mjs
```

## Skills Available

All agents in this project have access to these skills:

| Skill | When to use |
|-------|-------------|
| `qml-qt-quick-reference` | Writing or debugging QML, Qt Quick Controls, Layouts, JS |
| `kde-plasma-api` | PlasmoidItem, Kirigami, PlasmaComponents, config system |
| `kde-dev-tools` | kpackagetool6, plasmawindowed, qmllint, debugging, paths |
| `kde-plasmoid-workflow` | Project-specific install/test/verify workflow |
| `codexbar-cli-bridge` | CLI data contract, helper script, provider data shape |
