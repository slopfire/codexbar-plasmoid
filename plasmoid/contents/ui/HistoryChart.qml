import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

Item {
    id: chart

    property var points: []
    property color accentColor: Kirigami.Theme.highlightColor

    readonly property var values: (points || []).map(function(point) {
        const cost = Number(point && point.costUSD);
        if (Number.isFinite(cost) && cost > 0) {
            return cost;
        }
        const tokens = Number(point && point.totalTokens);
        return Number.isFinite(tokens) && tokens > 0 ? tokens : 0;
    })
    readonly property real maxValue: {
        let max = 0;
        for (const value of chart.values) {
            if (Number.isFinite(value) && value > max) {
                max = value;
            }
        }
        return max;
    }

    RowLayout {
        anchors.fill: parent
        spacing: 2

        Repeater {
            model: chart.points || []

            Item {
                id: dayCell

                required property var modelData
                required property int index

                Layout.fillWidth: true
                Layout.fillHeight: true

                readonly property real value: {
                    const list = chart.values;
                    const item = list && list.length > index ? list[index] : 0;
                    return Number.isFinite(item) ? item : 0;
                }
                readonly property bool hasUsage: dayCell.value > 0
                readonly property bool hasLimitReset: {
                    const resets = modelData && modelData.limitResets;
                    return Array.isArray(resets) && resets.length > 0;
                }

                PlasmaComponents3.ToolTip.delay: 250
                PlasmaComponents3.ToolTip.visible: dayHover.hovered && PlasmaComponents3.ToolTip.text !== ""
                PlasmaComponents3.ToolTip.text: chart.formatDayTooltip(modelData)

                HoverHandler {
                    id: dayHover
                }

                // Transparent hit target for the full column height.
                Rectangle {
                    anchors.fill: parent
                    color: dayHover.hovered
                        ? Qt.rgba(chart.accentColor.r, chart.accentColor.g, chart.accentColor.b, 0.12)
                        : "transparent"
                    radius: 2
                }

                // Usage bar. Zero-usage days stay a flat baseline tick.
                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: 0
                    anchors.rightMargin: 0
                    height: {
                        if (!dayCell.hasUsage) {
                            return 2;
                        }
                        if (chart.maxValue <= 0) {
                            return 2;
                        }
                        return Math.max(2, parent.height * (dayCell.value / chart.maxValue));
                    }
                    radius: Math.max(1, width / 2)
                    color: chart.accentColor
                    opacity: dayCell.hasUsage ? (dayHover.hovered ? 0.95 : 0.72) : 0.28
                }

                // Marker for an unused limit reset landing on this day.
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 1
                    width: Math.max(3, Math.min(parent.width - 1, 5))
                    height: width
                    radius: width / 2
                    visible: dayCell.hasLimitReset
                    color: Kirigami.Theme.neutralTextColor
                    opacity: dayHover.hovered ? 1 : 0.85
                    border.width: 1
                    border.color: Qt.rgba(Kirigami.Theme.backgroundColor.r, Kirigami.Theme.backgroundColor.g, Kirigami.Theme.backgroundColor.b, 0.6)
                }
            }
        }
    }

    function formatDayTooltip(point) {
        if (!point) {
            return "";
        }

        const lines = [];
        lines.push(formatDayTitle(point.dayKey));

        const cost = Number(point.costUSD);
        const tokens = Number(point.totalTokens);
        const hasCost = Number.isFinite(cost) && cost > 0;
        const hasTokens = Number.isFinite(tokens) && tokens > 0;

        if (hasCost || hasTokens) {
            const parts = [];
            if (hasCost) {
                parts.push("$" + Number(cost).toLocaleString(Qt.locale(), "f", cost >= 10 ? 2 : 4));
            }
            if (hasTokens) {
                parts.push(formatTokenCount(tokens) + " tokens");
            }
            lines.push(parts.join(" · "));
        } else {
            lines.push(i18n("No usage"));
        }

        const models = Array.isArray(point.models) ? point.models : [];
        if (models.length > 0) {
            lines.push("");
            lines.push(i18n("Models"));
            for (const model of models) {
                const name = String(model.name || model.modelName || "model");
                const modelCost = Number(model.costUSD);
                const modelTokens = Number(model.totalTokens);
                const bits = [];
                if (Number.isFinite(modelCost) && modelCost > 0) {
                    bits.push("$" + Number(modelCost).toLocaleString(Qt.locale(), "f", modelCost >= 10 ? 2 : 4));
                }
                if (Number.isFinite(modelTokens) && modelTokens > 0) {
                    bits.push(formatTokenCount(modelTokens));
                }
                lines.push(bits.length > 0 ? (name + ": " + bits.join(" · ")) : name);
            }
        }

        const resets = Array.isArray(point.limitResets) ? point.limitResets : [];
        if (resets.length > 0) {
            lines.push("");
            lines.push(i18n("Unused limit resets"));
            for (const reset of resets) {
                const title = String(reset.title || i18n("Limit"));
                const left = Number(reset.percentLeft);
                const leftText = Number.isFinite(left) ? Math.round(left) + "%" : "—";
                const when = formatResetWhen(reset.resetsAt, point.dayKey);
                lines.push(title + ": " + i18n("%1 unused", leftText) + (when ? " · " + when : ""));
            }
        }

        return lines.join("\n");
    }

    function formatDayTitle(dayKey) {
        if (!dayKey) {
            return i18n("Day");
        }
        const parts = String(dayKey).split("-");
        if (parts.length !== 3) {
            return String(dayKey);
        }
        const year = Number(parts[0]);
        const month = Number(parts[1]);
        const day = Number(parts[2]);
        if (!Number.isFinite(year) || !Number.isFinite(month) || !Number.isFinite(day)) {
            return String(dayKey);
        }
        const date = new Date(year, month - 1, day);
        if (!Number.isFinite(date.getTime())) {
            return String(dayKey);
        }
        return Qt.formatDate(date, Qt.DefaultLocaleLongDate);
    }

    function formatTokenCount(value) {
        const n = Number(value);
        if (!Number.isFinite(n) || n <= 0) {
            return "0";
        }
        if (n >= 1_000_000) {
            return Number(n / 1_000_000).toLocaleString(Qt.locale(), "f", n >= 10_000_000 ? 1 : 2) + "M";
        }
        if (n >= 1_000) {
            return Number(n / 1_000).toLocaleString(Qt.locale(), "f", n >= 10_000 ? 0 : 1) + "K";
        }
        return Number(n).toLocaleString(Qt.locale(), "f", 0);
    }

    function formatResetWhen(resetsAt, dayKey) {
        if (!resetsAt) {
            return i18n("resets this day");
        }
        const date = new Date(resetsAt);
        if (!Number.isFinite(date.getTime())) {
            return i18n("resets this day");
        }
        const todayKey = Qt.formatDate(new Date(), "yyyy-MM-dd");
        const resetDayKey = Qt.formatDate(date, "yyyy-MM-dd");
        if (resetDayKey === todayKey) {
            return i18n("resets today at %1", Qt.formatTime(date, Qt.DefaultLocaleShortDate));
        }
        if (dayKey === resetDayKey) {
            return i18n("resets at %1", Qt.formatTime(date, Qt.DefaultLocaleShortDate));
        }
        // Upcoming reset shown on today's hover (reset day may be outside the series).
        const diffMs = date.getTime() - Date.now();
        if (diffMs <= 0) {
            return i18n("resetting…");
        }
        const totalMinutes = Math.floor(diffMs / 60000);
        const days = Math.floor(totalMinutes / (60 * 24));
        const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
        if (days > 0) {
            return i18n("resets in %1d %2h (%3)", days, hours, Qt.formatDate(date, Qt.DefaultLocaleShortDate));
        }
        if (hours > 0) {
            return i18n("resets in %1h (%2)", hours, Qt.formatTime(date, Qt.DefaultLocaleShortDate));
        }
        return i18n("resets soon (%1)", Qt.formatTime(date, Qt.DefaultLocaleShortDate));
    }
}
