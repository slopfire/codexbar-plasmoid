import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

ColumnLayout {
    id: row

    property string title: ""
    property real percentLeft: 0
    property string resetsAt: ""
    property color accentColor: Kirigami.Theme.highlightColor
    // Fill color: white when full, muted yellow mid, red when low (red by ~10%).
    readonly property color remainingColor: {
        const value = Number(percentLeft);
        if (!Number.isFinite(value)) {
            return accentColor;
        }
        const t = Math.max(0, Math.min(100, value)) / 100;
        // muted yellow around 55% remaining; pure red by 10%
        const yellowAt = 0.55;
        const redAt = 0.10;
        // soft butter yellow (not pure/neon)
        const yR = 1.0, yG = 0.92, yB = 0.45;
        if (t <= redAt) {
            return Qt.rgba(1, 0, 0, 1);
        }
        if (t >= yellowAt) {
            // white (1,1,1) → muted yellow
            const u = (t - yellowAt) / (1 - yellowAt);
            return Qt.rgba(1, 1 - (1 - yG) * (1 - u), 1 - (1 - yB) * (1 - u), 1);
        }
        // muted yellow → red (1,0,0)
        const u = (t - redAt) / (yellowAt - redAt);
        return Qt.rgba(1, yG * u, yB * u, 1);
    }

    spacing: Kirigami.Units.smallSpacing / 2

    PlasmaComponents3.ToolTip.delay: Qt.styleHints.mousePressAndHoldInterval
    PlasmaComponents3.ToolTip.visible: hoverHandler.hovered && PlasmaComponents3.ToolTip.text !== ""
    PlasmaComponents3.ToolTip.text: formatResetTime(row.resetsAt)

    HoverHandler {
        id: hoverHandler
    }

    function formatResetTime(value) {
        if (!value) {
            return "";
        }
        const date = new Date(value);
        if (!Number.isFinite(date.getTime())) {
            return "";
        }
        const diffMs = date.getTime() - Date.now();
        if (diffMs <= 0) {
            return i18n("Resetting...");
        }
        const totalSeconds = Math.floor(diffMs / 1000);
        const days = Math.floor(totalSeconds / 86400);
        const hours = Math.floor((totalSeconds % 86400) / 3600);
        const minutes = Math.floor((totalSeconds % 3600) / 60);

        const parts = [];
        if (days > 0) {
            parts.push(i18np("%1 day", "%1 days", days));
        }
        if (hours > 0 || days > 0) {
            parts.push(i18np("%1 hour", "%1 hours", hours));
        }
        if (days === 0 || minutes > 0) {
            parts.push(i18np("%1 minute", "%1 minutes", minutes));
        }

        return i18n("Resets in %1", parts.join(" "));
    }

    RowLayout {
        Layout.fillWidth: true

        PlasmaComponents3.Label {
            Layout.fillWidth: true
            text: row.title
            font: Kirigami.Theme.smallFont
            elide: Text.ElideRight
        }

        PlasmaComponents3.Label {
            text: Number.isFinite(Number(row.percentLeft)) ? Math.round(Number(row.percentLeft)) + "%" : "—"
            color: Kirigami.Theme.disabledTextColor
            font: Kirigami.Theme.smallFont
        }
    }

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: Kirigami.Units.smallSpacing
        radius: height / 2
        color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.09)

        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * Math.max(0, Math.min(100, Number(row.percentLeft))) / 100
            radius: parent.radius
            color: row.remainingColor
        }
    }

    PlasmaComponents3.Label {
        Layout.fillWidth: true
        visible: row.resetsAt.length > 0 && formatResetTime(row.resetsAt) !== ""
        text: formatResetTime(row.resetsAt)
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        elide: Text.ElideRight
    }
}
