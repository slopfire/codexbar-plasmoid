---
name: kde-plasma-api
description: >-
  Use when working with KDE Plasma 6 widget APIs, Kirigami components, Plasma
  theming, PlasmoidItem, PlasmaComponents 3, PlasmaExtras, configuration
  (main.xml / config.qml), metadata.json, package structure, DataSource
  process execution, or migrating from Plasma 5 to 6. Covers all Plasma QML
  imports and the KConfigXT schema.
---

# KDE Plasma 6 & Kirigami API Reference

## Plasma 6 QML Imports

```qml
import org.kde.plasma.plasmoid                       // PlasmoidItem, Plasmoid
import org.kde.plasma.core as PlasmaCore              // Theme, Types, Action
import org.kde.plasma.components 3.0 as PlasmaComponents3  // Buttons, Labels, etc.
import org.kde.plasma.extras as PlasmaExtras          // Heading, Representation
import org.kde.plasma.plasma5support as Plasma5Support // DataSource
import org.kde.kirigami as Kirigami                   // Theme, Units, Icon, etc.
```

## PlasmoidItem — Widget Root Element

Every Plasma 6 plasmoid root must be `PlasmoidItem`, not `Item` or `Rectangle`:

```qml
PlasmoidItem {
    id: root

    // --- Representations ---
    compactRepresentation: CompactRep {}     // Shown in panel
    fullRepresentation: FullRep {}           // Shown when expanded / windowed

    // Auto-switch based on form factor
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar
        ? fullRepresentation : compactRepresentation

    // --- Tooltip ---
    toolTipMainText: "Widget Title"
    toolTipSubText: "Status info"

    // --- Context Menu ---
    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: i18n("Refresh")
            icon.name: "view-refresh"
            onTriggered: refresh()
        }
    ]

    Component.onCompleted: refreshNow()
}
```

### Critical Plasma 6 Rules

- Assign `compactRepresentation`, `fullRepresentation`, `preferredRepresentation`,
  `toolTipMainText`, `toolTipSubText` **directly on PlasmoidItem**, not as
  `Plasmoid.fullRepresentation`.
- Root QML must use `PlasmoidItem`, never plain `Item`.

## Plasmoid Context Object

Available anywhere inside PlasmoidItem:

```qml
Plasmoid.configuration.refreshIntervalSeconds  // Read config values
Plasmoid.configuration.cliPath
Plasmoid.formFactor     // PlasmaCore.Types.Planar, Horizontal, Vertical
Plasmoid.location       // PlasmaCore.Types.TopEdge, BottomEdge, etc.
Plasmoid.title          // Widget title from metadata.json
Plasmoid.icon           // Widget icon from metadata.json
```

## PlasmaCore.Types — Form Factors

```qml
PlasmaCore.Types.Planar       // Desktop / plasmawindowed
PlasmaCore.Types.Horizontal   // Horizontal panel
PlasmaCore.Types.Vertical     // Vertical panel
PlasmaCore.Types.Application  // Application mode
```

## PlasmaComponents 3

```qml
import org.kde.plasma.components 3.0 as PlasmaComponents3

PlasmaComponents3.Label { text: "Hello"; elide: Text.ElideRight }
PlasmaComponents3.ToolButton { icon.name: "view-refresh"; onClicked: refresh() }
PlasmaComponents3.TabBar {
    PlasmaComponents3.TabButton { text: "Tab 1" }
    PlasmaComponents3.TabButton { text: "Tab 2" }
}
PlasmaComponents3.BusyIndicator { running: loading }
PlasmaComponents3.ProgressBar { from: 0; to: 100; value: percent }
PlasmaComponents3.ScrollView { /* content */ }
PlasmaComponents3.Switch { text: "Toggle" }
PlasmaComponents3.TextField { placeholderText: "Search…" }
PlasmaComponents3.ItemDelegate { text: "Item" }
```

## PlasmaExtras

```qml
import org.kde.plasma.extras as PlasmaExtras

PlasmaExtras.Heading { text: "Title"; level: 2 }

PlasmaExtras.Representation {
    header: PlasmaExtras.PlasmoidHeading {
        RowLayout {
            PlasmaExtras.Heading { text: "Widget"; level: 2 }
            Item { Layout.fillWidth: true }
            PlasmaComponents3.ToolButton { icon.name: "view-refresh" }
        }
    }
    // contentItem is the body
}

PlasmaExtras.ScrollArea { ColumnLayout { /* scrollable content */ } }
```

---

## Kirigami

### Theme Colors

```qml
Kirigami.Theme.textColor
Kirigami.Theme.backgroundColor
Kirigami.Theme.highlightColor
Kirigami.Theme.highlightedTextColor
Kirigami.Theme.activeTextColor
Kirigami.Theme.linkColor
Kirigami.Theme.positiveTextColor       // Green/success
Kirigami.Theme.neutralTextColor        // Yellow/warning
Kirigami.Theme.negativeTextColor       // Red/error
Kirigami.Theme.disabledTextColor
Kirigami.Theme.alternateBackgroundColor
```

### Units

```qml
Kirigami.Units.smallSpacing            // ~4px, tight gaps
Kirigami.Units.largeSpacing            // ~8–12px, section gaps
Kirigami.Units.gridUnit                // Base grid unit (~18px)
Kirigami.Units.iconSizes.small         // 16px
Kirigami.Units.iconSizes.smallMedium   // 22px
Kirigami.Units.iconSizes.medium        // 32px
Kirigami.Units.iconSizes.large         // 48px
Kirigami.Units.iconSizes.huge          // 64px
Kirigami.Units.longDuration            // ~200ms animation
Kirigami.Units.shortDuration           // ~100ms animation
```

### Components

```qml
Kirigami.Icon {
    source: "preferences-system"
    implicitWidth: Kirigami.Units.iconSizes.small
    implicitHeight: Kirigami.Units.iconSizes.small
    color: Kirigami.Theme.textColor
}

Kirigami.Heading {
    text: "Section"
    level: 2   // 1=largest, 6=smallest
}

Kirigami.Separator {}

Kirigami.InlineMessage {
    type: Kirigami.MessageType.Warning
    text: "Needs attention"
    visible: showWarning
    actions: [
        Kirigami.Action { text: "Fix"; onTriggered: fix() }
    ]
}
```

### FormLayout (Config Pages)

```qml
Kirigami.FormLayout {
    QQC2.TextField {
        Kirigami.FormData.label: i18n("Name:")
        text: cfg_name
    }
    QQC2.CheckBox {
        Kirigami.FormData.label: i18n("Options:")
        text: i18n("Enable feature")
    }
    Kirigami.Separator {
        Kirigami.FormData.isSection: true
        Kirigami.FormData.label: i18n("Advanced")
    }
    QQC2.SpinBox {
        Kirigami.FormData.label: i18n("Count:")
        from: 1; to: 100
    }
}
```

### ScrollablePage

```qml
Kirigami.ScrollablePage {
    id: page
    property alias cfg_myProp: control.value
    Kirigami.FormLayout { /* config fields */ }
}
```

---

## Configuration System

### main.xml (KConfigXT Schema)

Location: `plasmoid/contents/config/main.xml`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<kcfg xmlns="http://www.kde.org/standards/kcfg/1.0"
      xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
      xsi:schemaLocation="http://www.kde.org/standards/kcfg/1.0
        https://www.kde.org/standards/kcfg/1.0/kcfg.xsd">
  <kcfgfile name=""/>
  <group name="General">
    <entry name="refreshInterval" type="Int">
      <label>Refresh interval in seconds</label>
      <default>120</default>
      <min>30</min>
      <max>86400</max>
    </entry>
    <entry name="showDetails" type="Bool">
      <default>true</default>
    </entry>
    <entry name="apiKey" type="String">
      <default></default>
    </entry>
  </group>
</kcfg>
```

Supported types: `String`, `Int`, `Bool`, `Double`, `StringList`, `Url`, `Color`,
`Font`, `Enum`, `Path`.

### config.qml (Entrypoint)

Location: `plasmoid/contents/config/config.qml`

```qml
import QtQuick
import org.kde.plasma.configuration

ConfigModel {
    ConfigCategory {
        name: i18n("General")
        icon: "configure"
        source: "configGeneral.qml"
    }
}
```

### Config Page Auto-Binding Pattern

In config page QML, `cfg_` prefixed properties auto-bind to
`Plasmoid.configuration.<name>`:

```qml
Kirigami.ScrollablePage {
    property alias cfg_refreshInterval: slider.value
    property alias cfg_showDetails: checkbox.checked
    property string cfg_apiKey   // non-alias for manual binding

    Kirigami.FormLayout {
        QQC2.Slider {
            id: slider
            Kirigami.FormData.label: i18n("Refresh interval:")
            from: 30; to: 3600; stepSize: 30
        }
        QQC2.CheckBox {
            id: checkbox
            Kirigami.FormData.label: i18n("Options:")
            text: i18n("Show details")
        }
    }
}
```

---

## Package Structure

```
plasmoid/
  metadata.json
  contents/
    code/              # Helper scripts (.mjs), executables
    config/
      config.qml       # Configuration entrypoint (ConfigModel)
      main.xml         # KConfigXT schema
    images/            # Icons, logos
    ui/
      main.qml         # Main widget QML (PlasmoidItem root)
      configGeneral.qml
      *.qml            # Additional components
```

### metadata.json

```json
{
  "KPackageStructure": "Plasma/Applet",
  "KPlugin": {
    "Authors": [{ "Name": "Author" }],
    "Category": "System Information",
    "Description": "Widget description",
    "Icon": "utilities-system-monitor",
    "Id": "org.example.mywidget",
    "License": "MIT",
    "Name": "My Widget",
    "Version": "1.0.0"
  },
  "X-Plasma-API-Minimum-Version": "6.0"
}
```

Required fields: `KPackageStructure: "Plasma/Applet"`, `KPlugin.Id`,
`X-Plasma-API-Minimum-Version: "6.0"`.

---

## Process Execution — DataSource

```qml
import org.kde.plasma.plasma5support as Plasma5Support

Plasma5Support.DataSource {
    id: executable
    engine: "executable"
    connectedSources: []
    onNewData: (source, data) => {
        let stdout = data["stdout"];
        let stderr = data["stderr"];
        let exitCode = data["exit code"];
        if (exitCode === 0) processResult(JSON.parse(stdout));
        disconnectSource(source);
    }
    function exec(cmd) { connectSource(cmd); }
}
// Usage: executable.exec("mycommand --json")
```

---

## Plasma 5 → 6 Migration

| Plasma 5 | Plasma 6 |
|-----------|----------|
| `import org.kde.plasma.core 2.0` | `import org.kde.plasma.core as PlasmaCore` |
| `import org.kde.plasma.components 2.0` | `import org.kde.plasma.components 3.0 as PlasmaComponents3` |
| `import org.kde.plasma.plasmoid 2.0` | `import org.kde.plasma.plasmoid` |
| `import org.kde.kirigami 2.20` | `import org.kde.kirigami as Kirigami` |
| `PlasmaCore.DataSource` | `import org.kde.plasma.plasma5support as P5S; P5S.DataSource` |
| `Plasmoid.fullRepresentation: Item {}` | `fullRepresentation: Item {}` on PlasmoidItem |
| `Plasmoid.compactRepresentation` | `compactRepresentation:` on PlasmoidItem |
| `Plasmoid.preferredRepresentation` | `preferredRepresentation:` on PlasmoidItem |
| `Plasmoid.toolTipMainText` | `toolTipMainText:` on PlasmoidItem |
| `plasmoid.nativeInterface` | Removed; use DataSource or helpers |
| `PlasmaCore.Theme.textColor` | `Kirigami.Theme.textColor` |
| `Units.smallSpacing` | `Kirigami.Units.smallSpacing` |
| `metadata.desktop` | `metadata.json` |

## i18n (Internationalization)

```qml
i18n("Simple string")
i18n("Hello %1", userName)
i18np("One item", "%1 items", count)
i18nc("@label", "Context-specific translation")
```

## Common Icon Names

Find more with `cuttlefish` (KDE icon browser):

```
view-refresh    configure         go-previous      go-next
dialog-ok       dialog-cancel     dialog-warning   dialog-error
dialog-information  list-add      list-remove      edit-delete
system-shutdown     edit-copy     document-save    document-open
preferences-system  arrow-up     arrow-down        chronometer
office-chart-bar    utilities-system-monitor
```

## References

- [KDE Developer Docs](https://develop.kde.org)
- [Plasma Widget Tutorial](https://develop.kde.org/docs/plasma/)
- [Kirigami Gallery](https://apps.kde.org/kirigami2.gallery/)
