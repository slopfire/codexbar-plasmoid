import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import QtQml.Models
import org.kde.kirigami as Kirigami

Item {
    id: switcher

    signal entrySelected(string entryId)
    signal orderCommitted(var orderedEntries)

    property var entries: []
    property var selectedEntryIds: []
    property var colorForProvider: null
    property bool reorderEnabled: true
    // Parent overlay while a chip is dragged (usually the full representation).
    property Item dragContainer: switcher
    property bool reorderActive: false

    // Report height to ColumnLayout so the expanded popup can size to content
    // when the chip grid wraps to extra rows. Collapse fully when hidden —
    // visible:false alone still reserves layout space.
    readonly property real contentHeight: entryModel.count > 1 ? grid.implicitHeight : 0
    implicitHeight: contentHeight
    Layout.preferredHeight: contentHeight
    Layout.minimumHeight: contentHeight
    Layout.maximumHeight: contentHeight
    height: contentHeight
    visible: entryModel.count > 1

    readonly property bool canReorder: reorderEnabled && entryModel.count > 1

    readonly property real buttonWidth: {
        let maxW = Kirigami.Units.gridUnit * 4.6;
        for (let i = 0; i < entryModel.count; ++i) {
            const name = providerName(entryModel.get(i).provider);
            maxW = Math.max(maxW, name.length * Kirigami.Theme.defaultFont.pixelSize * 0.64 + Kirigami.Units.largeSpacing * 2);
        }
        return Math.ceil(maxW);
    }

    readonly property int gridColumns: {
        if (entryModel.count <= 1) {
            return 1;
        }
        const available = width > 0 ? width : Kirigami.Units.gridUnit * 24;
        const maxCols = Math.max(1, Math.floor((available + Kirigami.Units.smallSpacing) / (buttonWidth + Kirigami.Units.smallSpacing)));
        return Math.min(entryModel.count, maxCols);
    }

    ListModel {
        id: entryModel
    }

    onEntriesChanged: Qt.callLater(syncEntryModel)
    Component.onCompleted: syncEntryModel()

    function entryJsonForModel(entry) {
        try {
            return JSON.stringify(entry || {});
        } catch (error) {
            return "{}";
        }
    }

    function entryFromModelItem(item) {
        try {
            return JSON.parse(String(item.entryJson || "{}"));
        } catch (error) {
            return null;
        }
    }

    function syncEntryModel() {
        if (reorderActive) {
            return;
        }
        const list = entries || [];
        let sameOrder = entryModel.count === list.length;
        if (sameOrder) {
            for (let index = 0; index < list.length; index += 1) {
                if (String(entryModel.get(index).entryId || "") !== String(list[index].id || "")) {
                    sameOrder = false;
                    break;
                }
            }
        }
        if (sameOrder) {
            for (let index = 0; index < list.length; index += 1) {
                const entry = list[index];
                entryModel.set(index, {
                    entryId: String(entry.id || ""),
                    provider: String(entry.provider || ""),
                    entryJson: entryJsonForModel(entry)
                });
            }
            return;
        }
        entryModel.clear();
        for (let index = 0; index < list.length; index += 1) {
            const entry = list[index];
            entryModel.append({
                entryId: String(entry.id || ""),
                provider: String(entry.provider || ""),
                entryJson: entryJsonForModel(entry)
            });
        }
    }

    function orderedEntriesFromModel() {
        const ordered = [];
        for (let index = 0; index < entryModel.count; index += 1) {
            const entry = entryFromModelItem(entryModel.get(index));
            if (entry) {
                ordered.push(entry);
            }
        }
        return ordered;
    }

    function commitOrder() {
        const ordered = orderedEntriesFromModel();
        reorderActive = true;
        orderCommitted(ordered);
        reorderActive = false;
        Qt.callLater(syncEntryModel);
    }

    GridLayout {
        id: grid
        anchors.horizontalCenter: parent.horizontalCenter
        columns: switcher.gridColumns
        rowSpacing: Kirigami.Units.smallSpacing
        columnSpacing: Kirigami.Units.smallSpacing

        Repeater {
            model: DelegateModel {
                id: visualModel
                model: entryModel

                delegate: DropArea {
                    id: dropDelegate

                    required property int index
                    required property string entryId
                    required property string provider
                    required property string entryJson

                    property int visualIndex: DelegateModel.itemsIndex
                    readonly property var entryData: {
                        try {
                            return JSON.parse(entryJson || "{}");
                        } catch (error) {
                            return null;
                        }
                    }

                    Layout.preferredWidth: switcher.buttonWidth
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 3.4
                    width: switcher.buttonWidth
                    height: Kirigami.Units.gridUnit * 3.4

                    keys: ["codexbar-provider-chip"]

                    onEntered: function(drag) {
                        if (!switcher.canReorder) {
                            return;
                        }
                        if (!drag.source || drag.source.visualIndex === undefined) {
                            return;
                        }
                        const fromIndex = drag.source.visualIndex;
                        const toIndex = dropDelegate.visualIndex;
                        if (fromIndex === toIndex || fromIndex < 0 || toIndex < 0) {
                            return;
                        }
                        switcher.reorderActive = true;
                        entryModel.move(fromIndex, toIndex, 1);
                    }

                    Item {
                        id: chip

                        property int visualIndex: dropDelegate.visualIndex
                        property bool dragActive: dragArea.drag.active
                        property bool suppressClick: false

                        width: dropDelegate.width
                        height: dropDelegate.height
                        // Use individual center anchors so AnchorChanges can clear
                        // them while dragging (centerIn is not supported there).
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.verticalCenter: parent.verticalCenter
                        z: dragActive ? 100 : 0
                        opacity: dragActive ? 0.92 : 1.0
                        scale: dragActive ? 1.04 : 1.0
                        Behavior on scale { NumberAnimation { duration: 100 } }
                        Behavior on opacity { NumberAnimation { duration: 100 } }

                        Drag.active: dragActive && switcher.canReorder
                        Drag.source: chip
                        Drag.keys: ["codexbar-provider-chip"]
                        Drag.hotSpot.x: width / 2
                        Drag.hotSpot.y: height / 2

                        states: [
                            State {
                                when: chip.dragActive
                                ParentChange {
                                    target: chip
                                    parent: switcher.dragContainer || switcher
                                }
                                AnchorChanges {
                                    target: chip
                                    anchors {
                                        horizontalCenter: undefined
                                        verticalCenter: undefined
                                    }
                                }
                            }
                        ]

                        onDragActiveChanged: {
                            if (dragActive) {
                                switcher.reorderActive = true;
                                chip.suppressClick = true;
                                return;
                            }
                            switcher.commitOrder();
                        }

                        Rectangle {
                            anchors.fill: parent
                            radius: Kirigami.Units.cornerRadius
                            readonly property color accent: switcher.providerColor(dropDelegate.provider)
                            readonly property bool selected: switcher.isEntrySelected(dropDelegate.entryData)
                            color: selected
                                ? Qt.rgba(accent.r, accent.g, accent.b, 0.16)
                                : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, dragArea.containsMouse ? 0.07 : 0.025)
                            border.width: 1
                            border.color: selected
                                ? Qt.rgba(accent.r, accent.g, accent.b, 0.62)
                                : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.10)
                        }

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: Kirigami.Units.smallSpacing
                            spacing: Kirigami.Units.smallSpacing / 2

                            QtControls.Label {
                                Layout.fillWidth: true
                                text: switcher.providerName(dropDelegate.provider)
                                color: Kirigami.Theme.textColor
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                font.bold: switcher.isEntrySelected(dropDelegate.entryData)
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: Kirigami.Units.smallSpacing
                                visible: !switcher.hasBalance(dropDelegate.entryData)
                                radius: height / 2
                                color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.11)

                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.top: parent.top
                                    anchors.bottom: parent.bottom
                                    width: parent.width * switcher.primaryPercent(dropDelegate.entryData) / 100
                                    radius: parent.radius
                                    color: switcher.providerColor(dropDelegate.provider)
                                }
                            }

                            QtControls.Label {
                                Layout.fillWidth: true
                                visible: switcher.hasBalance(dropDelegate.entryData)
                                text: switcher.balanceText(dropDelegate.entryData)
                                color: switcher.providerColor(dropDelegate.provider)
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                font.bold: true
                                font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                            }

                            QtControls.Label {
                                Layout.fillWidth: true
                                text: (dropDelegate.entryData && dropDelegate.entryData.account)
                                    || (dropDelegate.entryData && dropDelegate.entryData.source)
                                    || ""
                                color: Kirigami.Theme.disabledTextColor
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                font: Kirigami.Theme.smallFont
                            }
                        }

                        MouseArea {
                            id: dragArea
                            anchors.fill: parent
                            hoverEnabled: true
                            preventStealing: switcher.canReorder
                            acceptedButtons: Qt.LeftButton
                            cursorShape: {
                                if (!switcher.canReorder) {
                                    return Qt.PointingHandCursor;
                                }
                                if (drag.active) {
                                    return Qt.ClosedHandCursor;
                                }
                                return containsMouse ? Qt.OpenHandCursor : Qt.ArrowCursor;
                            }
                            drag.target: switcher.canReorder ? chip : undefined
                            drag.axis: Drag.XAndYAxis
                            drag.threshold: Kirigami.Units.smallSpacing

                            onPressed: {
                                chip.suppressClick = false;
                            }

                            onClicked: {
                                // Skip click after a drag so selection is not toggled while reordering.
                                if (chip.suppressClick || drag.active) {
                                    return;
                                }
                                switcher.entrySelected(dropDelegate.entryId);
                            }
                        }

                        QtControls.ToolTip {
                            visible: dragArea.containsMouse && !chip.dragActive && switcher.canReorder
                            text: i18n("Click to select · drag to reorder")
                            delay: 700
                        }
                    }
                }
            }
        }
    }

    function isEntrySelected(entry) {
        if (!entry || !entry.id) {
            return false;
        }
        const selected = selectedEntryIds || [];
        if (selected.indexOf(entry.id) !== -1) {
            return true;
        }
        // Soft-match restored tokens after Plasma restart when the id shape
        // changed (legacy index ids, provider-only fallback, account drift).
        const provider = String(entry.provider || "");
        for (let i = 0; i < selected.length; i += 1) {
            const token = String(selected[i] || "");
            if (!token) {
                continue;
            }
            if (token === provider) {
                return true;
            }
            const colon = token.indexOf(":");
            const tokenProvider = colon === -1 ? token : token.slice(0, colon);
            if (tokenProvider !== provider) {
                continue;
            }
            const rest = colon === -1 ? "" : token.slice(colon + 1);
            if (!rest) {
                return true;
            }
            if (entry.account === rest || entry.source === rest) {
                return true;
            }
        }
        return false;
    }

    function primaryPercent(entry) {
        const rows = entry && entry.rows ? entry.rows : [];
        if (rows.length === 0) {
            return entry && (entry.creditsRemaining !== null || entry.tokenUsage) ? 100 : 0;
        }
        const value = Number(rows[0].percentLeft);
        return Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
    }

    function hasBalance(entry) {
        const rows = entry && entry.rows ? entry.rows : [];
        return !!(entry && rows.length === 0 && entry.creditsRemaining !== null);
    }

    function balanceText(entry) {
        if (!hasBalance(entry)) {
            return "";
        }
        return "USD " + Number(entry.creditsRemaining).toLocaleString(Qt.locale(), "f", entry.creditsRemaining >= 100 ? 0 : 2);
    }

    function providerName(provider) {
        const names = {
            codex: "Codex",
            openai: "OpenAI",
            azureopenai: "Azure OpenAI",
            claude: "Claude",
            cursor: "Cursor",
            gemini: "Gemini",
            copilot: "Copilot",
            antigravity: "Antigravity",
            opencode: "OpenCode",
            opencodego: "OpenCode Go",
            minimax: "MiniMax",
            grok: "Grok",
            groq: "GroqCloud",
            openrouter: "OpenRouter",
            devin: "Devin"
        };
        return names[provider] || String(provider || "");
    }

    function providerColor(provider) {
        if (typeof switcher.colorForProvider === "function") {
            return switcher.colorForProvider(provider);
        }
        const colors = {
            codex: "#4b929b",
            claude: "#b57861",
            cursor: "#3c9487",
            gemini: "#8972b5",
            copilot: "#8c68b7",
            openai: "#398979",
            minimax: "#bd684e",
            grok: "#3d8d76",
            groq: "#b86c55",
            openrouter: "#4d88ad",
            devin: "#6769ad"
        };
        return colors[provider] || Kirigami.Theme.highlightColor;
    }
}
