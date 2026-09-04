import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

PlasmaComponents3.Frame {
    id: card

    property var entry
    property string providerName: ""
    property url providerSiteUrl: ""
    property color accentColor: Kirigami.Theme.highlightColor
    property bool showCredits: true
    property bool showHistory: true

    signal siteRequested()

    padding: Kirigami.Units.smallSpacing

    // Collapse to a compact error notice when the entry has an error and no
    // usage data to render (e.g. an expired OpenCode Go session in a
    // secondary browser profile). Keeps the dashboard readable when several
    // accounts of the same provider surface failures.
    readonly property bool isErrorOnly: !!(
        entry && entry.error
        && (!entry.rows || entry.rows.length === 0)
        && !showBalanceSummary()
    )
    opacity: isErrorOnly ? 0.72 : 1.0

    background: Rectangle {
        color: "transparent"
        border.width: 0
    }

    // contentItem so Frame/Control reports a real implicitHeight for ListView
    // content-sizing (children alone are ignored by Control's size calculation).
    contentItem: ColumnLayout {
        id: contentLayout
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            ProviderIcon {
                id: providerIcon
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                source: {
                    const id = String(card.entry ? card.entry.provider : "").toLowerCase().replace(/[-_]/g, "");
                    const known = ["abacus", "alibaba", "amp", "antigravity", "augment", "bedrock", "claude", "codebuff", "codex", "commandcode", "copilot", "crof", "cursor", "deepgram", "deepseek", "demo", "devin", "doubao", "elevenlabs", "factory", "gemini", "grok", "groq", "jetbrains", "kilo", "kimi", "kiro", "llmproxy", "manus", "mimo", "minimax", "mistral", "ollama", "opencode", "opencodego", "openrouter", "perplexity", "stepfun", "synthetic", "t3chat", "venice", "vertexai", "warp", "windsurf", "zai"];
                    if (known.includes(id)) {
                        return Qt.resolvedUrl("../images/ProviderIcon-" + id + ".svg");
                    }
                    return Qt.resolvedUrl("../images/ProviderIcon-codex.svg");
                }
                color: card.accentColor

                MouseArea {
                    id: providerIconMouseArea
                    anchors.fill: parent
                    enabled: String(card.providerSiteUrl).length > 0
                    hoverEnabled: enabled
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: card.siteRequested()

                    QtControls.ToolTip {
                        visible: providerIconMouseArea.containsMouse
                        text: i18n("Open %1 website", card.providerName)
                    }
                }
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
            visible: !card.isErrorOnly
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
            visible: !card.isErrorOnly && card.entry && card.entry.codeReviewRemainingPercent !== null
            title: i18n("Code review")
            percentLeft: card.entry ? card.entry.codeReviewRemainingPercent : null
            accentColor: card.accentColor
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Kirigami.Units.smallSpacing
            Layout.bottomMargin: Kirigami.Units.smallSpacing
            visible: !card.isErrorOnly && card.showBalanceSummary()
            spacing: Kirigami.Units.largeSpacing

            Rectangle {
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: parent.height
                Layout.minimumHeight: Kirigami.Units.gridUnit * 2.4
                radius: width / 2
                color: card.accentColor
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.primarySummaryLabel()
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                }

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.primarySummaryValue()
                    font.bold: true
                    font.pointSize: Kirigami.Theme.defaultFont.pointSize + 2
                    elide: Text.ElideRight
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                visible: card.secondarySummaryValue().length > 0
                spacing: 0

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.secondarySummaryLabel()
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                    horizontalAlignment: Text.AlignRight
                }

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.secondarySummaryValue()
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                    horizontalAlignment: Text.AlignRight
                }
            }
        }

        GridLayout {
            Layout.fillWidth: true
            visible: !card.isErrorOnly
            columns: width > Kirigami.Units.gridUnit * 18 ? 2 : 1
            rowSpacing: Kirigami.Units.smallSpacing
            columnSpacing: Kirigami.Units.largeSpacing

            MetricLine {
                Layout.fillWidth: true
                visible: card.showCredits && card.entry && card.entry.creditsRemaining !== null && !card.showBalanceSummary()
                title: i18n("Credits")
                value: card.entry ? Number(card.entry.creditsRemaining).toLocaleString(Qt.locale(), "f", 2) : "—"
            }

            // Stacked in one grid cell so "Reset limits" / "Expires" share a
            // label column and values line up (not interleaved with Today/30d).
            GridLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                visible: card.showLimitResetCredits()
                columns: 2
                columnSpacing: Kirigami.Units.smallSpacing
                rowSpacing: Math.max(1, Math.round(Kirigami.Units.smallSpacing / 2))

                PlasmaComponents3.Label {
                    text: i18n("Reset limits")
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                }
                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: card.limitResetCreditsValue()
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                }

                PlasmaComponents3.Label {
                    visible: card.limitResetExpiresValue().length > 0
                    text: i18n("Expires")
                    color: Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                }
                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    visible: card.limitResetExpiresValue().length > 0
                    text: card.limitResetExpiresValue()
                    font: Kirigami.Theme.smallFont
                    elide: Text.ElideRight
                }
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

        Loader {
            id: historyLoader

            Layout.fillWidth: true
            // Collapse fully when inactive so content height tracks real cards.
            Layout.preferredHeight: active ? Kirigami.Units.gridUnit * 3.5 : 0
            active: !card.isErrorOnly
                && card.showHistory
                && card.entry
                && card.entry.dailyUsage
                && card.entry.dailyUsage.length > 0
            visible: active

            sourceComponent: Component {
                HistoryChart {
                    points: card.entry && card.entry.dailyUsage ? card.entry.dailyUsage : []
                    accentColor: card.accentColor
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: card.isErrorOnly ? 0 : Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing
            visible: card.entry && card.entry.error

            Kirigami.Icon {
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                source: "dialog-warning"
                color: Kirigami.Theme.negativeTextColor
                visible: card.isErrorOnly
            }

            PlasmaComponents3.Label {
                Layout.fillWidth: true
                text: card.entry && card.entry.error ? (card.entry.error.message || card.entry.error.description || JSON.stringify(card.entry.error)) : ""
                color: Kirigami.Theme.negativeTextColor
                font: card.isErrorOnly ? Kirigami.Theme.defaultFont : Kirigami.Theme.smallFont
                wrapMode: Text.Wrap
                maximumLineCount: card.isErrorOnly ? -1 : 3
                elide: Text.ElideRight
            }
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

    function hasUsageRows() {
        return !!(entry && entry.rows && entry.rows.length > 0);
    }

    function showBalanceSummary() {
        return !!(entry && !hasUsageRows() && (entry.creditsRemaining !== null || entry.tokenUsage));
    }

    function primarySummaryLabel() {
        if (!entry) {
            return "";
        }
        if (entry.creditsRemaining !== null) {
            return i18n("Balance");
        }
        return entry.tokenUsage ? entry.tokenUsage.sessionLabel : "";
    }

    function primarySummaryValue() {
        if (!entry) {
            return "—";
        }
        if (entry.creditsRemaining !== null) {
            return money(entry.creditsRemaining, entry.tokenUsage ? entry.tokenUsage.currencyCode : "USD");
        }
        return entry.tokenUsage ? money(entry.tokenUsage.sessionCostUSD, entry.tokenUsage.currencyCode) : "—";
    }

    function secondarySummaryLabel() {
        if (!entry || !entry.tokenUsage) {
            return "";
        }
        return entry.creditsRemaining !== null ? entry.tokenUsage.sessionLabel : entry.tokenUsage.last30DaysLabel;
    }

    function secondarySummaryValue() {
        if (!entry || !entry.tokenUsage) {
            return "";
        }
        const cost = entry.creditsRemaining !== null ? entry.tokenUsage.sessionCostUSD : entry.tokenUsage.last30DaysCostUSD;
        const tokens = entry.creditsRemaining !== null ? entry.tokenUsage.sessionTokens : entry.tokenUsage.last30DaysTokens;
        const tokenPart = tokenText(tokens);
        return money(cost, entry.tokenUsage.currencyCode) + (tokenPart.length > 0 ? " · " + tokenPart : "");
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

    function showLimitResetCredits() {
        return !!(entry && entry.limitResetCredits && Number.isFinite(Number(entry.limitResetCredits.availableCount)));
    }

    function limitResetCreditsValue() {
        if (!showLimitResetCredits()) {
            return "";
        }
        const count = Math.max(0, Math.round(Number(entry.limitResetCredits.availableCount)));
        return i18np("%1 available", "%1 available", count);
    }

    function limitResetExpiresValue() {
        if (!showLimitResetCredits()) {
            return "";
        }
        const expiresAt = entry.limitResetCredits.nextExpiresAt;
        if (!expiresAt) {
            return "";
        }
        const date = new Date(expiresAt);
        if (!Number.isFinite(date.getTime())) {
            return "";
        }
        const diffMs = date.getTime() - Date.now();
        if (diffMs <= 0) {
            return i18n("soon");
        }
        const totalMinutes = Math.floor(diffMs / 60000);
        const days = Math.floor(totalMinutes / (60 * 24));
        const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
        // Value-only phrasing; the row title is already "Expires".
        if (days > 0) {
            return i18n("in %1d %2h", days, hours);
        }
        if (hours > 0) {
            const minutes = totalMinutes % 60;
            return i18n("in %1h %2m", hours, minutes);
        }
        return i18n("in %1m", Math.max(1, totalMinutes));
    }
}
