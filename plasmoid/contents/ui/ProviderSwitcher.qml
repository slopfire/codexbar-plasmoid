import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami

Item {
    id: switcher

    signal entrySelected(string entryId)

    property var entries: []
    property string selectedEntryId: ""

    implicitHeight: grid.implicitHeight
    height: grid.implicitHeight
    visible: entries.length > 1

    readonly property real buttonWidth: {
        let maxW = Kirigami.Units.gridUnit * 4.6;
        for (let i = 0; i < entries.length; ++i) {
            const name = providerName(entries[i].provider);
            maxW = Math.max(maxW, name.length * Kirigami.Theme.defaultFont.pixelSize * 0.64 + Kirigami.Units.largeSpacing * 2);
        }
        return Math.ceil(maxW);
    }

    readonly property int gridColumns: {
        if (entries.length <= 1) {
            return 1;
        }
        const available = width > 0 ? width : Kirigami.Units.gridUnit * 24;
        const maxCols = Math.max(1, Math.floor((available + Kirigami.Units.smallSpacing) / (buttonWidth + Kirigami.Units.smallSpacing)));
        return Math.min(entries.length, maxCols);
    }

    GridLayout {
        id: grid
        anchors.horizontalCenter: parent.horizontalCenter
        columns: switcher.gridColumns
        rowSpacing: Kirigami.Units.smallSpacing
        columnSpacing: Kirigami.Units.smallSpacing

        Repeater {
            model: switcher.entries

            delegate: QtControls.AbstractButton {
                id: button

                required property var modelData

                Layout.preferredWidth: switcher.buttonWidth
                Layout.preferredHeight: Kirigami.Units.gridUnit * 3.4
                checkable: true
                checked: switcher.selectedEntryId === button.modelData.id
                onClicked: switcher.entrySelected(button.modelData.id)

                background: Rectangle {
                    radius: Kirigami.Units.cornerRadius
                    color: button.checked
                        ? Kirigami.Theme.highlightColor
                        : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, button.hovered ? 0.10 : 0.04)
                    border.width: button.checked ? 0 : 1
                    border.color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.10)
                }

                contentItem: ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing / 2

                    QtControls.Label {
                        id: title
                        Layout.fillWidth: true
                        text: switcher.providerName(button.modelData.provider)
                        color: button.checked ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font.bold: button.checked
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Kirigami.Units.smallSpacing
                        visible: !switcher.hasBalance(button.modelData)
                        radius: height / 2
                        color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, button.checked ? 0.18 : 0.12)

                        Rectangle {
                            anchors.left: parent.left
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: parent.width * switcher.primaryPercent(button.modelData) / 100
                            radius: parent.radius
                            color: button.checked ? Kirigami.Theme.highlightedTextColor : switcher.providerColor(button.modelData.provider)
                        }
                    }

                    QtControls.Label {
                        Layout.fillWidth: true
                        visible: switcher.hasBalance(button.modelData)
                        text: switcher.balanceText(button.modelData)
                        color: button.checked ? Kirigami.Theme.highlightedTextColor : switcher.providerColor(button.modelData.provider)
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font.bold: true
                        font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                    }

                    QtControls.Label {
                        Layout.fillWidth: true
                        text: button.modelData.account || button.modelData.source || ""
                        color: button.checked
                            ? Qt.rgba(Kirigami.Theme.highlightedTextColor.r, Kirigami.Theme.highlightedTextColor.g, Kirigami.Theme.highlightedTextColor.b, 0.78)
                            : Kirigami.Theme.disabledTextColor
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                    }
                }
            }
        }
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
            openrouter: "OpenRouter"
        };
        return names[provider] || String(provider || "");
    }

    function providerColor(provider) {
        const colors = {
            codex: "#49a3b0",
            claude: "#cc7c5e",
            cursor: "#00bfa5",
            gemini: "#ab87ea",
            copilot: "#a855f7",
            openai: "#0f826e",
            minimax: "#fe603c",
            grok: "#10a37f",
            groq: "#f56844",
            openrouter: "#3da3d9"
        };
        return colors[provider] || Kirigami.Theme.highlightColor;
    }
}
