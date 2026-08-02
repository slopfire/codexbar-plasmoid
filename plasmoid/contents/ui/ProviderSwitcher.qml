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
    // "tabs"  = compact pill buttons (icon + full label)
    // "chips" = card grid with usage bar / account (legacy)
    // "none"  = hide the switcher entirely
    property string style: "tabs"
    // Parent overlay while a chip is dragged (usually the full representation).
    property Item dragContainer: switcher
    property bool reorderActive: false

    readonly property bool isTabsStyle: style === "tabs"
    readonly property bool isChipsStyle: style === "chips"
    readonly property bool isHidden: style === "none" || style === ""
    readonly property bool hasMultiple: entryModel.count > 1
    readonly property bool switcherVisible: hasMultiple && !isHidden

    // Report height to ColumnLayout so the expanded popup can size to content
    // when the chip grid wraps to extra rows. Collapse fully when hidden —
    // visible:false alone still reserves layout space.
    readonly property real contentHeight: switcherVisible ? layoutRoot.implicitHeight : 0
    implicitHeight: contentHeight
    Layout.preferredHeight: contentHeight
    Layout.minimumHeight: contentHeight
    Layout.maximumHeight: contentHeight
    height: contentHeight
    visible: switcherVisible

    readonly property bool canReorder: reorderEnabled && hasMultiple

    // Cards: equal grid cells; content is name + bar/balance + account.
    readonly property real chipMinWidth: {
        let maxW = Kirigami.Units.gridUnit * 4.6;
        for (let i = 0; i < entryModel.count; ++i) {
            const name = providerName(entryModel.get(i).provider);
            maxW = Math.max(maxW, name.length * Kirigami.Theme.defaultFont.pixelSize * 0.64 + Kirigami.Units.largeSpacing * 2);
        }
        return Math.ceil(maxW);
    }
    readonly property real chipSpacing: Kirigami.Units.smallSpacing
    readonly property real chipRowHeight: Kirigami.Units.gridUnit * 3.4

    readonly property real tabRowHeight: Math.max(
        Kirigami.Units.gridUnit * 1.75,
        Kirigami.Theme.defaultFont.pixelSize + Kirigami.Units.smallSpacing * 3
    )
    readonly property real tabIconSize: Kirigami.Units.iconSizes.small
    readonly property real tabHPad: Kirigami.Units.smallSpacing + 6
    readonly property real tabIconGap: Kirigami.Units.smallSpacing
    readonly property real tabSpacing: Kirigami.Units.largeSpacing

    TextMetrics {
        id: tabTextMetrics
        font: Kirigami.Theme.defaultFont
    }

    function tabMinWidthFor(provider) {
        const name = providerName(provider);
        const boldFont = Qt.font({
            family: Kirigami.Theme.defaultFont.family,
            pixelSize: Kirigami.Theme.defaultFont.pixelSize,
            weight: Font.Bold
        });
        tabTextMetrics.font = boldFont;
        tabTextMetrics.text = name;
        const textW = Math.ceil(Math.max(tabTextMetrics.advanceWidth, tabTextMetrics.boundingRect.width));
        return Math.ceil(tabIconSize + tabIconGap + textW + tabHPad * 2 + 2);
    }

    function visualModelItemAt(index) {
        if (visualModel.items.count > index) {
            return visualModel.items.get(index).model;
        }
        return index >= 0 && index < entryModel.count ? entryModel.get(index) : null;
    }

    function providerAtVisualIndex(index) {
        const item = visualModelItemAt(index);
        return item ? String(item.provider || "") : "";
    }

    // Manual wrap layout (one DelegateModel, absolute positions):
    // - tabs:  content min-width, flex-grow to fill each row (edge to edge)
    // - cards: equal-width grid columns, left-aligned (like the reference)
    property var itemGeom: []
    property real canvasHeight: 0
    property int layoutGeneration: 0

    readonly property real buttonHeight: isTabsStyle ? tabRowHeight : chipRowHeight
    readonly property real layoutSpacing: isTabsStyle ? tabSpacing : chipSpacing

    onWidthChanged: Qt.callLater(recomputeLayout)
    onStyleChanged: Qt.callLater(recomputeLayout)
    onTabSpacingChanged: Qt.callLater(recomputeLayout)

    function recomputeLayout() {
        const count = entryModel.count;
        if (count <= 0 || isHidden) {
            itemGeom = [];
            canvasHeight = 0;
            layoutGeneration += 1;
            return;
        }
        const available = Math.max(1, width > 0 ? width : Kirigami.Units.gridUnit * 24);
        const gap = layoutSpacing;
        const h = buttonHeight;
        const geoms = new Array(count);

        if (isChipsStyle) {
            // Equal columns from min card width; last row left-aligned under the grid.
            const cols = Math.max(
                1,
                Math.min(count, Math.floor((available + gap) / (chipMinWidth + gap)))
            );
            const cellW = Math.max(
                chipMinWidth,
                Math.floor((available - gap * Math.max(0, cols - 1)) / cols)
            );
            for (let i = 0; i < count; ++i) {
                const col = i % cols;
                const row = Math.floor(i / cols);
                geoms[i] = {
                    x: col * (cellW + gap),
                    y: row * (h + gap),
                    w: cellW
                };
            }
            const rows = Math.ceil(count / cols);
            canvasHeight = rows * h + Math.max(0, rows - 1) * gap;
            itemGeom = geoms;
            layoutGeneration += 1;
            return;
        }

        // Tabs: pack by content min-width, flex-grow each row full width.
        const mins = [];
        for (let i = 0; i < count; ++i) {
            mins.push(tabMinWidthFor(providerAtVisualIndex(i)));
        }
        let index = 0;
        let y = 0;
        while (index < count) {
            const rowStart = index;
            let used = 0;
            let cols = 0;
            while (index < count) {
                const nextUsed = used + mins[index] + (cols > 0 ? gap : 0);
                if (cols > 0 && nextUsed > available) {
                    break;
                }
                used = nextUsed;
                cols += 1;
                index += 1;
            }
            if (cols === 0) {
                cols = 1;
                used = mins[index];
                index += 1;
            }
            const free = Math.max(0, available - used);
            const extra = free / cols;
            const widths = new Array(cols);
            for (let c = 0; c < cols; ++c) {
                widths[c] = Math.floor(mins[rowStart + c] + extra);
            }
            let sum = gap * Math.max(0, cols - 1);
            for (let c = 0; c < cols; ++c) {
                sum += widths[c];
            }
            widths[cols - 1] += available - sum;
            let x = 0;
            for (let c = 0; c < cols; ++c) {
                const i = rowStart + c;
                geoms[i] = { x: x, y: y, w: widths[c] };
                x += widths[c] + gap;
            }
            y += h + gap;
        }
        itemGeom = geoms;
        canvasHeight = Math.max(0, y - gap);
        layoutGeneration += 1;
    }

    function geomAt(index) {
        // layoutGeneration forces dependents to re-read after recompute.
        const _gen = layoutGeneration;
        if (index >= 0 && index < itemGeom.length && itemGeom[index]) {
            return itemGeom[index];
        }
        return { x: 0, y: 0, w: Kirigami.Units.gridUnit * 4 };
    }

    function itemX(index) { return geomAt(index).x; }
    function itemY(index) { return geomAt(index).y; }
    function itemW(index) { return geomAt(index).w; }

    readonly property var knownProviderIconIds: [
        "abacus", "alibaba", "amp", "antigravity", "augment", "bedrock", "claude",
        "codebuff", "codex", "commandcode", "copilot", "crof", "cursor", "deepgram",
        "deepseek", "demo", "devin", "doubao", "elevenlabs", "factory", "gemini", "grok",
        "groq", "jetbrains", "kilo", "kimi", "kiro", "llmproxy", "manus", "mimo",
        "minimax", "mistral", "ollama", "opencode", "opencodego", "openrouter",
        "perplexity", "stepfun", "synthetic", "t3chat", "venice", "vertexai",
        "warp", "windsurf", "zai"
    ]

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
            Qt.callLater(recomputeLayout);
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
        Qt.callLater(recomputeLayout);
    }

    function orderedEntriesFromVisualModel() {
        const ordered = [];
        for (let index = 0; index < visualModel.items.count; index += 1) {
            const entry = entryFromModelItem(visualModelItemAt(index));
            if (entry) {
                ordered.push(entry);
            }
        }
        return ordered;
    }

    function commitOrder() {
        const ordered = orderedEntriesFromVisualModel();
        reorderActive = true;
        orderCommitted(ordered);
        reorderActive = false;
        Qt.callLater(syncEntryModel);
    }

    // Single canvas + single DelegateModel. Positions are precomputed so each
    // wrapped row is centered (Flow would left-align).
    Item {
        id: layoutRoot
        anchors.left: parent.left
        anchors.right: parent.right
        implicitHeight: switcher.canvasHeight
        height: implicitHeight

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
                    readonly property bool selected: switcher.isEntrySelected(entryData)
                    readonly property color accent: switcher.providerColor(provider)
                    readonly property int layoutIndex: visualIndex >= 0 ? visualIndex : index

                    x: switcher.itemX(layoutIndex)
                    y: switcher.itemY(layoutIndex)
                    width: switcher.itemW(layoutIndex)
                    height: switcher.buttonHeight

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
                        // DelegateModel.itemsIndex belongs to the visual group,
                        // not to the backing ListModel. Moving the source model
                        // with that index can desynchronize the two orders after
                        // a resize. Keep drag order visual until commitOrder()
                        // persists it to the source snapshot.
                        visualModel.items.move(fromIndex, toIndex);
                        switcher.recomputeLayout();
                    }

                        Item {
                            id: chip

                            property int visualIndex: dropDelegate.visualIndex
                            property bool dragActive: dragArea.drag.active
                            property bool dragWasActive: false
                            property bool suppressClick: false

                            width: dropDelegate.width
                            height: dropDelegate.height
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
                                    chip.dragWasActive = true;
                                    chip.suppressClick = true;
                                    return;
                                }
                                if (chip.dragWasActive) {
                                    chip.dragWasActive = false;
                                    switcher.commitOrder();
                                }
                            }

                            Rectangle {
                                anchors.fill: parent
                                radius: switcher.isTabsStyle ? height / 2 : Kirigami.Units.cornerRadius
                                color: {
                                    if (switcher.isTabsStyle) {
                                        if (dropDelegate.selected) {
                                            return Qt.rgba(
                                                Kirigami.Theme.textColor.r,
                                                Kirigami.Theme.textColor.g,
                                                Kirigami.Theme.textColor.b,
                                                0.16
                                            );
                                        }
                                        return Qt.rgba(
                                            Kirigami.Theme.textColor.r,
                                            Kirigami.Theme.textColor.g,
                                            Kirigami.Theme.textColor.b,
                                            dragArea.containsMouse ? 0.10 : 0.06
                                        );
                                    }
                                    return dropDelegate.selected
                                        ? Qt.rgba(dropDelegate.accent.r, dropDelegate.accent.g, dropDelegate.accent.b, 0.16)
                                        : Qt.rgba(
                                            Kirigami.Theme.textColor.r,
                                            Kirigami.Theme.textColor.g,
                                            Kirigami.Theme.textColor.b,
                                            dragArea.containsMouse ? 0.07 : 0.025
                                        );
                                }
                                border.width: 1
                                border.color: {
                                    if (switcher.isTabsStyle) {
                                        return dropDelegate.selected
                                            ? Qt.rgba(
                                                Kirigami.Theme.textColor.r,
                                                Kirigami.Theme.textColor.g,
                                                Kirigami.Theme.textColor.b,
                                                0.28
                                            )
                                            : Qt.rgba(
                                                Kirigami.Theme.textColor.r,
                                                Kirigami.Theme.textColor.g,
                                                Kirigami.Theme.textColor.b,
                                                0.12
                                            );
                                    }
                                    return dropDelegate.selected
                                        ? Qt.rgba(dropDelegate.accent.r, dropDelegate.accent.g, dropDelegate.accent.b, 0.62)
                                        : Qt.rgba(
                                            Kirigami.Theme.textColor.r,
                                            Kirigami.Theme.textColor.g,
                                            Kirigami.Theme.textColor.b,
                                            0.10
                                        );
                                }

                                Behavior on color { ColorAnimation { duration: 120 } }
                            }

                            // Tabs: icon + full label
                            Row {
                                visible: switcher.isTabsStyle
                                anchors.centerIn: parent
                                spacing: switcher.tabIconGap

                                Kirigami.Icon {
                                    width: switcher.tabIconSize
                                    height: switcher.tabIconSize
                                    anchors.verticalCenter: parent.verticalCenter
                                    source: switcher.providerIconSource(dropDelegate.provider)
                                    isMask: true
                                    color: Kirigami.Theme.textColor
                                }

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: switcher.providerName(dropDelegate.provider)
                                    color: Kirigami.Theme.textColor
                                    font.bold: dropDelegate.selected
                                    font.pixelSize: Kirigami.Theme.defaultFont.pixelSize
                                }
                            }

                            // Cards: name + bar/balance + account (original look)
                            ColumnLayout {
                                visible: switcher.isChipsStyle
                                anchors.fill: parent
                                anchors.margins: Kirigami.Units.smallSpacing
                                spacing: Kirigami.Units.smallSpacing / 2

                                QtControls.Label {
                                    Layout.fillWidth: true
                                    text: switcher.providerName(dropDelegate.provider)
                                    color: Kirigami.Theme.textColor
                                    horizontalAlignment: Text.AlignHCenter
                                    elide: Text.ElideRight
                                    font.bold: dropDelegate.selected
                                }

                                Rectangle {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: Kirigami.Units.smallSpacing
                                    visible: !switcher.hasBalance(dropDelegate.entryData)
                                    radius: height / 2
                                    color: Qt.rgba(
                                        Kirigami.Theme.textColor.r,
                                        Kirigami.Theme.textColor.g,
                                        Kirigami.Theme.textColor.b,
                                        0.11
                                    )

                                    Rectangle {
                                        anchors.left: parent.left
                                        anchors.top: parent.top
                                        anchors.bottom: parent.bottom
                                        width: parent.width * switcher.primaryPercent(dropDelegate.entryData) / 100
                                        radius: parent.radius
                                        color: dropDelegate.accent
                                    }
                                }

                                QtControls.Label {
                                    Layout.fillWidth: true
                                    visible: switcher.hasBalance(dropDelegate.entryData)
                                    text: switcher.balanceText(dropDelegate.entryData)
                                    color: dropDelegate.accent
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
            devin: "Devin",
            jetbrains: "Jetbrains",
            factory: "Factory",
            kilo: "Kilo",
            kiro: "Kiro",
            ollama: "Ollama",
            windsurf: "Windsurf",
            perplexity: "Perplexity",
            mistral: "Mistral",
            kimi: "Kimi",
            kimik2: "Kimi K2",
            augment: "Augment",
            zai: "z.ai",
            "z.ai": "z.ai"
        };
        const key = String(provider || "").toLowerCase().replace(/[-_]/g, "");
        return names[key] || names[provider] || String(provider || "");
    }

    function providerIconSource(provider) {
        const id = String(provider || "").toLowerCase().replace(/[-_]/g, "");
        if (knownProviderIconIds.indexOf(id) !== -1) {
            return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
        }
        return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
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
            devin: "#6769ad",
            jetbrains: "#6b8cae"
        };
        return colors[provider] || Kirigami.Theme.highlightColor;
    }
}
