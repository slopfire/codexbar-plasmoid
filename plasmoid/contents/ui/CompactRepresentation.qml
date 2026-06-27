import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

Control {
    id: compact

    Layout.minimumWidth: implicitWidth
    Layout.preferredWidth: implicitWidth
    Layout.maximumWidth: implicitWidth
    Layout.minimumHeight: implicitHeight
    Layout.preferredHeight: implicitHeight
    clip: true

    property var entry
    property bool loading: false
    property string errorText: ""
    property string providerId: ""
    property string providerName: "CodexBar"
    property color accentColor: Kirigami.Theme.highlightColor
    property string valueText: "—"
    property string displayMode: "icon"
    property bool showMetricText: true
    property var usageRows: []
    property var barItems: []
    property int providerBarWidth: 18

    readonly property bool showBars: compact.displayMode === "bars"
    readonly property url providerIconSource: {
        const known = ["abacus", "alibaba", "amp", "antigravity", "augment", "bedrock", "claude", "codebuff", "codex", "commandcode", "copilot", "crof", "cursor", "deepgram", "deepseek", "devin", "doubao", "elevenlabs", "factory", "gemini", "grok", "groq", "jetbrains", "kilo", "kimi", "kiro", "llmproxy", "manus", "mimo", "minimax", "mistral", "ollama", "opencode", "opencodego", "openrouter", "perplexity", "stepfun", "synthetic", "t3chat", "venice", "vertexai", "warp", "windsurf", "zai"];
        const id = String(compact.providerId || "").toLowerCase().replace(/[-_]/g, "");
        if (known.includes(id)) {
            return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
        }
        return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
    }

    readonly property bool isVertical: compact.width > 0 && compact.height > 0 && compact.width < compact.height
    readonly property bool showText: compact.showMetricText && compact.width > Kirigami.Units.gridUnit * 3 && !compact.isVertical
    readonly property real rowSpacing: compact.isVertical ? Kirigami.Units.smallSpacing : Kirigami.Units.largeSpacing

    readonly property real iconSize: compact.height > 0
        ? Math.max(16, Math.min(Kirigami.Units.iconSizes.smallMedium, Math.max(0, compact.availableHeight)))
        : Kirigami.Units.iconSizes.smallMedium
    readonly property real barGroupWidth: Math.max(8, Number(compact.providerBarWidth || 18))

    function creditsGroupWidth(text) {
        return Math.min(64, Math.max(compact.barGroupWidth, String(text).length * 6 + 4));
    }

    function barGroupsTotalWidth() {
        const items = compact.barItems || [];
        if (items.length === 0) {
            return compact.barGroupWidth;
        }
        let total = 0;
        for (let i = 0; i < items.length; i += 1) {
            const rows = items[i].rows || [];
            const isCredits = rows.length > 0 && rows[0].kind === "credits";
            total += isCredits
                ? compact.creditsGroupWidth(rows[0].valueText)
                : compact.barGroupWidth;
            if (i > 0) {
                total += Kirigami.Units.smallSpacing;
            }
        }
        return total;
    }

    readonly property real visualWidth: compact.showBars
        ? Math.max(compact.iconSize, compact.barGroupsTotalWidth())
        : compact.iconSize

    implicitWidth: Math.max(
        compact.showMetricText ? Kirigami.Units.gridUnit * 4.5 : 0,
        compact.visualWidth + leftPadding + rightPadding
            + (compact.showText ? compact.rowSpacing + valueLabel.implicitWidth + compact.rowSpacing : 0))

    // When the text column is hidden, force it to 0 width so the row does not
    // reserve phantom space (QML layouts include invisible items by default).
    readonly property real _textColumnReserved: compact.showText
        ? compact.rowSpacing + valueLabel.implicitWidth + compact.rowSpacing
        : 0
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
            Layout.minimumHeight: 0
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
                running: false
                visible: false
            }

            CompactUsageBars {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                visible: compact.errorText.length === 0 && compact.showBars
                usageRows: compact.usageRows
                barItems: compact.barItems
                accentColor: compact.accentColor
                barGroupWidth: compact.barGroupWidth
            }

            Kirigami.Icon {
                anchors.centerIn: parent
                width: parent.width
                height: parent.height
                visible: compact.errorText.length === 0 && !compact.showBars
                source: compact.providerIconSource
                isMask: true
                color: Kirigami.Theme.textColor
            }
        }

        ColumnLayout {
            visible: compact.showText
            Layout.alignment: Qt.AlignVCenter
            Layout.minimumWidth: compact.showText ? valueLabel.implicitWidth : 0
            Layout.preferredWidth: compact.showText ? valueLabel.implicitWidth : 0
            spacing: 0

            PlasmaComponents3.Label {
                id: valueLabel
                text: compact.loading && !compact.entry ? i18n("…") : compact.valueText
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
