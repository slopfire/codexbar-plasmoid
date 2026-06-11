import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

Control {
    id: compact

    property var entry
    property bool loading: false
    property string errorText: ""
    property string providerId: ""
    property string providerName: "CodexBar"
    property color accentColor: Kirigami.Theme.highlightColor
    property string valueText: "—"
    property string displayMode: "icon"
    property var usageRows: []
    property var barItems: []

    readonly property bool showBars: compact.displayMode === "bars"
    readonly property url providerIconSource: {
        const known = ["abacus", "alibaba", "amp", "antigravity", "augment", "bedrock", "claude", "codebuff", "codex", "commandcode", "copilot", "crof", "cursor", "deepgram", "deepseek", "doubao", "elevenlabs", "factory", "gemini", "grok", "groq", "jetbrains", "kilo", "kimi", "kiro", "llmproxy", "manus", "mimo", "minimax", "mistral", "ollama", "opencode", "opencodego", "openrouter", "perplexity", "stepfun", "synthetic", "t3chat", "venice", "vertexai", "warp", "windsurf", "zai"];
        const id = String(compact.providerId || "").toLowerCase().replace(/[-_]/g, "");
        if (known.includes(id)) {
            return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
        }
        return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
    }

    readonly property bool isVertical: compact.width > 0 && compact.height > 0 && compact.width < compact.height
    readonly property bool showText: compact.width > Kirigami.Units.gridUnit * 3 && !compact.isVertical
    readonly property real rowSpacing: compact.isVertical ? Kirigami.Units.smallSpacing : Kirigami.Units.largeSpacing

    readonly property real iconSize: compact.height > 0
        ? Math.max(16, Math.min(Kirigami.Units.iconSizes.smallMedium, Math.max(0, compact.availableHeight)))
        : Kirigami.Units.iconSizes.smallMedium
    readonly property real visualWidth: compact.showBars && compact.showText
        ? Math.max(compact.iconSize, Kirigami.Units.gridUnit * 2.75)
        : compact.iconSize

    implicitWidth: compact.isVertical
        ? compact.visualWidth + leftPadding + rightPadding
        : Math.max(Kirigami.Units.gridUnit * 4.5, compact.visualWidth + compact.rowSpacing + valueLabel.implicitWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(Kirigami.Units.iconSizes.small, contentItem.implicitHeight) + topPadding + bottomPadding
    leftPadding: compact.showText ? Kirigami.Units.largeSpacing : Math.round(Kirigami.Units.smallSpacing / 2)
    rightPadding: leftPadding
    topPadding: Math.round(Kirigami.Units.smallSpacing / 2)
    bottomPadding: topPadding

    contentItem: RowLayout {
        id: row
        spacing: compact.rowSpacing
        clip: true

        Item {
            id: visualSlot
            implicitWidth: compact.visualWidth
            implicitHeight: compact.iconSize
            width: implicitWidth
            height: implicitHeight
            Layout.preferredWidth: implicitWidth
            Layout.minimumWidth: implicitWidth
            Layout.preferredHeight: implicitHeight
            Layout.minimumHeight: implicitHeight
            Layout.alignment: Qt.AlignVCenter
            Layout.fillWidth: !compact.showText

            Kirigami.Icon {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                visible: compact.errorText.length > 0
                source: "data-warning"
                color: Kirigami.Theme.negativeTextColor
            }

            PlasmaComponents3.BusyIndicator {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                running: compact.loading
                visible: compact.loading && compact.errorText.length === 0
            }

            CompactUsageBars {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                visible: !compact.loading && compact.errorText.length === 0 && compact.showBars
                usageRows: compact.usageRows
                barItems: compact.barItems
                accentColor: compact.accentColor
            }

            Kirigami.Icon {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                visible: !compact.loading && compact.errorText.length === 0 && !compact.showBars
                source: compact.providerIconSource
                isMask: true
                color: Kirigami.Theme.textColor
            }
        }

        ColumnLayout {
            visible: compact.showText
            Layout.alignment: Qt.AlignVCenter
            Layout.minimumWidth: valueLabel.implicitWidth
            spacing: 0

            PlasmaComponents3.Label {
                id: valueLabel
                text: compact.loading ? i18n("…") : compact.valueText
                font.bold: true
                horizontalAlignment: Text.AlignLeft
                elide: Text.ElideRight
            }

            PlasmaComponents3.Label {
                Layout.fillWidth: true
                text: compact.providerName
                color: Kirigami.Theme.disabledTextColor
                font: Kirigami.Theme.smallFont
                elide: Text.ElideRight
                visible: compact.width > Kirigami.Units.gridUnit * 6 && compact.height >= 40
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: compact.clicked()
    }

    signal clicked()
}
