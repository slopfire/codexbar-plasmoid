---
name: qml-qt-quick-reference
description: >-
  Use when writing, reviewing, or debugging QML and JavaScript code. Covers Qt 6
  QML types, Qt Quick Controls 2, Layouts, JS integration (.mjs helpers),
  property bindings, signal handlers, common patterns/pitfalls, number/date
  formatting, and qmllint diagnostics. Applies to any *.qml or *.mjs file.
---

# QML & Qt Quick Reference (Qt 6 / Plasma 6)

## Qt 6 QML Import Style

Plasma 6 drops version numbers on most imports. Only `PlasmaComponents` keeps `3.0`:

```qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components 3.0 as PlasmaComponents3
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasmoid
import org.kde.plasma.plasma5support as Plasma5Support
```

## Properties

```qml
property string title: "Hello"
property int count: 0
property bool active: true
property var dynamicData: ({})          // untyped, use sparingly
property list<Item> childItems
readonly property int doubled: count * 2
required property string name          // caller must set
property alias labelText: label.text   // exposes nested prop
```

### Pitfalls

- **`var` vs typed**: prefer typed (`string`, `int`, `bool`, `real`, `color`).
- **`property alias`** works only for direct child properties; avoid chained aliases.
- **Binding loops**: `width: parent.width` when parent sizes to children causes a loop.
  Break with `Math.min(implicitWidth, parent.availableWidth)`.

## Core Visual Types

| Type | Key Props |
|------|-----------|
| `Item` | x, y, width, height, visible, opacity, anchors, z, clip |
| `Rectangle` | color, radius, border.width, border.color, gradient |
| `Text` | text, font, color, wrapMode, elide, horizontalAlignment |
| `Image` | source, fillMode, sourceSize, asynchronous |
| `Canvas` | onPaint, requestPaint(), getContext("2d") |
| `Flickable` | contentWidth, contentHeight, clip, interactive |

## Repeater / ListView / GridView

```qml
// Static/small — Repeater
Repeater {
    model: entries
    delegate: RowLayout {
        required property var modelData
        required property int index
        Label { text: modelData.name }
    }
}

// Large/scrollable — ListView (virtualized)
ListView {
    model: ListModel { ListElement { name: "A" } }
    delegate: ItemDelegate {
        required property string name
        text: name
    }
    spacing: 4
    clip: true
}
```

## Loader / Component

```qml
Loader {
    active: showHeavy
    sourceComponent: Component {
        Rectangle { /* expensive content */ }
    }
    onLoaded: item.initialize()
}
```

## MouseArea / TapHandler / Timer / Connections

```qml
MouseArea {
    anchors.fill: parent
    hoverEnabled: true
    onClicked: (mouse) => { doThing(mouse.x) }
    cursorShape: Qt.PointingHandCursor
}

Timer {
    id: refreshTimer
    interval: 120 * 1000
    repeat: true; running: true
    onTriggered: refresh()
}

Connections {
    target: someObj
    function onValueChanged(val) { handle(val) }
}
```

## Qt Quick Controls 2

Import as `QQC2` or `QtControls` to avoid name clashes:

```qml
import QtQuick.Controls as QQC2

QQC2.Button    { text: "OK"; icon.name: "dialog-ok"; onClicked: save() }
QQC2.Label     { text: "Desc"; wrapMode: Text.WordWrap }
QQC2.TextField { placeholderText: "Enter value…" }
QQC2.ComboBox  { model: ["A","B"]; onCurrentValueChanged: apply(currentValue) }
QQC2.CheckBox  { text: "Enable"; checked: cfg_enable }
QQC2.Switch    { text: "Dark mode" }
QQC2.Slider    { from: 0; to: 100; stepSize: 1; value: 50 }
QQC2.SpinBox   { from: 5; to: 300; value: 45 }
QQC2.ScrollView { contentWidth: availableWidth; ColumnLayout { /* … */ } }
QQC2.ToolTip   { text: "Hint"; delay: 500; timeout: 3000 }
QQC2.ToolButton { icon.name: "configure"; onClicked: openSettings() }

QQC2.Menu {
    QQC2.MenuItem { text: "Cut"; onTriggered: cut() }
}

QQC2.Dialog {
    title: "Confirm"
    standardButtons: Dialog.Ok | Dialog.Cancel
    onAccepted: doAction()
}

QQC2.Popup {
    modal: true
    anchors.centerIn: parent
}
```

## Qt Quick Layouts

```qml
import QtQuick.Layouts

RowLayout {
    spacing: Kirigami.Units.smallSpacing
    Label { text: "Name:" }
    TextField { Layout.fillWidth: true }
    Button { text: "Go" }
}

ColumnLayout {
    spacing: Kirigami.Units.largeSpacing
    Item { Layout.fillHeight: true }   // spacer
    Label { Layout.alignment: Qt.AlignHCenter }
}

GridLayout {
    columns: 2
    columnSpacing: 8; rowSpacing: 8
    Label { text: "Row 1" }
    TextField { Layout.fillWidth: true }
}
```

**Key attached properties**: `Layout.fillWidth`, `Layout.fillHeight`, `Layout.preferredWidth`,
`Layout.preferredHeight`, `Layout.minimumWidth`, `Layout.maximumWidth`, `Layout.alignment`,
`Layout.row`, `Layout.column`, `Layout.columnSpan`, `Layout.rowSpan`.

**Rule**: Never mix `anchors.fill` and `Layout.fillWidth` on the same item.

## JavaScript Integration

### Inline Functions

```qml
function processData(input) {
    let result = JSON.parse(input);
    return result.items.map(i => i.name).join(", ");
}
```

### ES Module Helpers (.mjs)

```qml
import "helper.mjs" as Helper
Component.onCompleted: { let data = Helper.process(rawData); }
```

```javascript
// helper.mjs
export function process(raw) {
    return JSON.parse(raw);
}
```

### XMLHttpRequest

```qml
function fetchData(url, callback) {
    let xhr = new XMLHttpRequest();
    xhr.onreadystatechange = function() {
        if (xhr.readyState === XMLHttpRequest.DONE && xhr.status === 200)
            callback(JSON.parse(xhr.responseText));
    };
    xhr.open("GET", url);
    xhr.send();
}
```

### WorkerScript (background thread)

```qml
WorkerScript {
    id: worker
    source: "worker.mjs"
    onMessage: (msg) => { resultModel.append(msg) }
}
```

## Number and Date Formatting (QML-Specific!)

```qml
// CORRECT — Qt locale API:
Number(value).toLocaleString(Qt.locale(), 'f', 2)

// WRONG — browser JS API (fails in QML with "Invalid arguments"):
value.toLocaleString('en-US', { maximumFractionDigits: 2 })

// Date formatting:
Qt.formatDateTime(new Date(), "yyyy-MM-dd HH:mm:ss")
Qt.formatDate(date, Qt.DefaultLocaleShortDate)
```

## Common Patterns

### Visibility Without Layout Collapse

```qml
visible: showPanel
// visible:false still occupies layout space. To fully collapse:
Layout.preferredHeight: showPanel ? implicitHeight : 0
```

### Anchors vs Layouts

- **Anchors**: pixel-level relative positioning. Good for static overlays.
- **Layouts**: flexible flow. Good for dynamic/responsive content.
- Never mix both on the same item.

### Signal Handlers

```qml
signal dataReady(var result)
onDataReady: (result) => { model = result }
```

### Component.onCompleted / onDestruction

```qml
Component.onCompleted: { initialize() }
Component.onDestruction: { cleanup() }
```

## Debugging

```qml
console.log("Value:", someProperty)
console.warn("Warning:", message)
console.error("Error:", error)
```

### qmllint

```bash
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
```

### Common Errors Quick Reference

| Error | Fix |
|-------|-----|
| `Cannot assign to non-existent property` | Wrong parent type or typo |
| `IDs cannot start with an uppercase letter` | Use `id: myItem` not `id: MyItem` |
| `Unable to assign [undefined]` | Null-check before use |
| `Binding loop detected` | Break circular dependency |
| `ReferenceError: X is not defined` | Out-of-scope id; pass via property |
| `Invalid arguments` in toLocaleString | Use Qt locale API, not browser JS |

## Qt.labs Modules

```qml
import Qt.labs.settings
Settings {
    property string username: "default"
}

import Qt.labs.platform
FileDialog {
    nameFilters: ["JSON (*.json)"]
    onAccepted: loadFile(file)
}
```
