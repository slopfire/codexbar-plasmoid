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

    readonly property bool showBars: compact.displayMode === "bars"
    readonly property url providerIconSource: {
        const known = ["codex", "claude", "gemini", "cursor", "opencode", "opencodego", "copilot"];
        const id = String(compact.providerId || "").toLowerCase();
        if (known.includes(id)) {
            return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
        }
        return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
    }

    readonly property real iconSize: compact.height > 0
        ? Math.max(16, Math.min(Kirigami.Units.iconSizes.smallMedium, Math.max(0, compact.availableHeight)))
        : Kirigami.Units.iconSizes.smallMedium

    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    implicitHeight: Math.max(Kirigami.Units.iconSizes.small, contentItem.implicitHeight) + topPadding + bottomPadding
    leftPadding: Kirigami.Units.smallSpacing
    rightPadding: Kirigami.Units.smallSpacing
    topPadding: Kirigami.Units.smallSpacing / 2
    bottomPadding: Kirigami.Units.smallSpacing / 2

    contentItem: RowLayout {
        id: row
        spacing: Kirigami.Units.smallSpacing
        clip: true

        Item {
            id: visualSlot
            width: compact.iconSize
            height: compact.iconSize
            Layout.preferredWidth: width
            Layout.minimumWidth: width
            Layout.preferredHeight: height
            Layout.minimumHeight: height
            Layout.alignment: Qt.AlignVCenter

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
