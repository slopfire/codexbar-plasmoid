import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: bars

    property var usageRows: []
    property color accentColor: Kirigami.Theme.highlightColor
    property bool stale: false

    readonly property real primaryPercent: percentAt(0)
    readonly property real secondaryPercent: percentAt(1)
    readonly property bool hasSecondary: usageRows.length > 1

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium

    function percentAt(index) {
        const rows = bars.usageRows || [];
        if (index < 0 || index >= rows.length) {
            return 0;
        }
        const value = Number(rows[index].percentLeft);
        return Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
    }

    readonly property color trackColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.18 : 0.28)
    readonly property color strokeColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.28 : 0.44)
    readonly property color fillColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.55 : 1.0)

    Rectangle {
        id: topTrack
        anchors.horizontalCenter: parent.horizontalCenter
        y: 2
        width: parent.width - 4
        height: bars.hasSecondary ? parent.height * 0.42 : parent.height - 4
        radius: height / 2
        color: bars.trackColor
        border.width: 1
        border.color: bars.strokeColor

        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * bars.primaryPercent / 100
            radius: parent.radius
            color: bars.fillColor
        }
    }

    Rectangle {
        id: bottomTrack
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 2
        width: parent.width - 4
        height: parent.height * 0.28
        visible: bars.hasSecondary
        radius: height / 2
        color: bars.trackColor
        border.width: 1
        border.color: bars.strokeColor

        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * bars.secondaryPercent / 100
            radius: parent.radius
            color: bars.fillColor
        }
    }
}
