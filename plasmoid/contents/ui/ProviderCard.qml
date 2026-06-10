import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

PlasmaComponents3.Frame {
    id: card

    property var entry
    property string providerName: ""
    property color accentColor: Kirigami.Theme.highlightColor
    property bool showCredits: true
    property bool showHistory: true

    padding: Kirigami.Units.smallSpacing

    background: Rectangle {
        color: "transparent"
        border.width: 0
    }

    ColumnLayout {
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true

            Kirigami.Icon {
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                source: {
                    const id = String(card.entry ? card.entry.provider : "").toLowerCase().replace(/[-_]/g, "");
                    const known = ["abacus", "alibaba", "amp", "antigravity", "augment", "bedrock", "claude", "codebuff", "codex", "commandcode", "copilot", "crof", "cursor", "deepgram", "deepseek", "doubao", "elevenlabs", "factory", "gemini", "grok", "groq", "jetbrains", "kilo", "kimi", "kiro", "llmproxy", "manus", "mimo", "minimax", "mistral", "ollama", "opencode", "opencodego", "openrouter", "perplexity", "stepfun", "synthetic", "t3chat", "venice", "vertexai", "warp", "windsurf", "zai"];
                    if (known.includes(id)) {
                        return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
                    }
                    return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
                }
                isMask: true
                color: card.accentColor
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.providerName
                    font.bold: true
                    elide: Text.ElideRight
                }

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: subtitle()
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                    visible: text.length > 0
                }
            }

            StatusBadge {
                status: card.entry ? card.entry.status : null
            }
        }

        Repeater {
            model: card.entry && card.entry.rows ? card.entry.rows : []

            UsageBarRow {
                Layout.fillWidth: true
                title: modelData.title
                percentLeft: modelData.percentLeft
                resetsAt: modelData.resetsAt || ""
                accentColor: card.accentColor
            }
        }

        UsageBarRow {
            Layout.fillWidth: true
            visible: card.entry && card.entry.codeReviewRemainingPercent !== null
            title: i18n("Code review")
            percentLeft: card.entry ? card.entry.codeReviewRemainingPercent : null
            accentColor: card.accentColor
        }

        GridLayout {
            Layout.fillWidth: true
            columns: width > Kirigami.Units.gridUnit * 18 ? 2 : 1
            rowSpacing: Kirigami.Units.smallSpacing
            columnSpacing: Kirigami.Units.largeSpacing

            MetricLine {
                Layout.fillWidth: true
                visible: card.showCredits && card.entry && card.entry.creditsRemaining !== null
                title: i18n("Credits")
                value: card.entry ? Number(card.entry.creditsRemaining).toLocaleString(Qt.locale(), "f", 2) : "—"
            }

            MetricLine {
                Layout.fillWidth: true
                visible: card.entry && card.entry.tokenUsage
                title: card.entry && card.entry.tokenUsage ? card.entry.tokenUsage.sessionLabel : i18n("Today")
                value: costAndTokens("session")
            }

            MetricLine {
                Layout.fillWidth: true
                visible: card.entry && card.entry.tokenUsage
                title: card.entry && card.entry.tokenUsage ? card.entry.tokenUsage.last30DaysLabel : i18n("30d")
                value: costAndTokens("last30")
            }
        }

        HistoryChart {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 3
            visible: card.showHistory && card.entry && card.entry.dailyUsage && card.entry.dailyUsage.length > 0
            points: card.entry && card.entry.dailyUsage ? card.entry.dailyUsage : []
            accentColor: card.accentColor
        }

        PlasmaComponents3.Label {
            Layout.fillWidth: true
            visible: card.entry && card.entry.error
            text: card.entry && card.entry.error ? (card.entry.error.message || card.entry.error.description || JSON.stringify(card.entry.error)) : ""
            color: Kirigami.Theme.negativeTextColor
            font: Kirigami.Theme.smallFont
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
        }

        PlasmaComponents3.Label {
            Layout.fillWidth: true
            text: card.footerInfo()
            color: Kirigami.Theme.disabledTextColor
            font: Kirigami.Theme.smallFont
            elide: Text.ElideRight
            visible: text.length > 0
            horizontalAlignment: Text.AlignRight
        }
    }

    function subtitle() {
        if (!entry) {
            return "";
        }
        const parts = [];
        if (entry.account) {
            parts.push(entry.account);
        }
        if (entry.organization) {
            parts.push(entry.organization);
        }
        if (entry.plan) {
            parts.push(entry.plan.indexOf("Plan:") === 0 ? entry.plan : i18n("Plan: %1", entry.plan));
        }
        return parts.join(" · ");
    }

    function footerInfo() {
        if (!entry) {
            return "";
        }
        const parts = [];
        if (entry.source) {
            parts.push(i18n("Source: %1", entry.source));
        }
        if (entry.version) {
            parts.push(i18n("Version: %1", entry.version));
        }
        return parts.join(" · ");
    }

    function money(value, code) {
        if (!Number.isFinite(Number(value))) {
            return "—";
        }
        return (code || "USD") + " " + Number(value).toLocaleString(Qt.locale(), "f", 2);
    }

    function tokenText(value) {
        if (!Number.isFinite(Number(value))) {
            return "";
        }
        return Math.round(Number(value)).toLocaleString(Qt.locale(), "f", 0) + " " + i18n("tokens");
    }

    function costAndTokens(kind) {
        if (!entry || !entry.tokenUsage) {
            return "—";
        }
        const token = entry.tokenUsage;
        const cost = kind === "session" ? token.sessionCostUSD : token.last30DaysCostUSD;
        const tokens = kind === "session" ? token.sessionTokens : token.last30DaysTokens;
        const tokenPart = tokenText(tokens);
        return money(cost, token.currencyCode) + (tokenPart.length > 0 ? " · " + tokenPart : "");
    }
}
