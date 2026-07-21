import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami

Item {
    id: switcher

    signal entrySelected(string entryId)

    property var entries: []
    property var selectedEntryIds: []
    property var colorForProvider: null

    // Report height to ColumnLayout so the expanded popup can size to content
    // when the chip grid wraps to extra rows. Collapse fully when hidden —
    // visible:false alone still reserves layout space.
    readonly property real contentHeight: entries.length > 1 ? grid.implicitHeight : 0
    implicitHeight: contentHeight
    Layout.preferredHeight: contentHeight
    Layout.minimumHeight: contentHeight
    Layout.maximumHeight: contentHeight
    height: contentHeight
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
                checked: switcher.isEntrySelected(button.modelData)
                onClicked: switcher.entrySelected(button.modelData.id)

                background: Rectangle {
                    radius: Kirigami.Units.cornerRadius
                    readonly property color accent: switcher.providerColor(button.modelData.provider)
                    color: button.checked
                        ? Qt.rgba(accent.r, accent.g, accent.b, 0.16)
                        : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, button.hovered ? 0.07 : 0.025)
                    border.width: 1
                    border.color: button.checked
                        ? Qt.rgba(accent.r, accent.g, accent.b, 0.62)
                        : Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.10)
                }

                contentItem: ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing / 2

                    QtControls.Label {
                        id: title
                        Layout.fillWidth: true
                        text: switcher.providerName(button.modelData.provider)
                        color: Kirigami.Theme.textColor
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font.bold: button.checked
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Kirigami.Units.smallSpacing
                        visible: !switcher.hasBalance(button.modelData)
                        radius: height / 2
                        color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.11)

                        Rectangle {
                            anchors.left: parent.left
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: parent.width * switcher.primaryPercent(button.modelData) / 100
                            radius: parent.radius
                            color: switcher.providerColor(button.modelData.provider)
                        }
                    }

                    QtControls.Label {
                        Layout.fillWidth: true
                        visible: switcher.hasBalance(button.modelData)
                        text: switcher.balanceText(button.modelData)
                        color: switcher.providerColor(button.modelData.provider)
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font.bold: true
                        font.pixelSize: Kirigami.Theme.smallFont.pixelSize
                    }

                    QtControls.Label {
                        Layout.fillWidth: true
                        text: button.modelData.account || button.modelData.source || ""
                        color: Kirigami.Theme.disabledTextColor
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
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
