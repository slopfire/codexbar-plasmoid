import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

ColumnLayout {
    id: row

    property string title: ""
    property real percentLeft: 0
    property color accentColor: Kirigami.Theme.highlightColor

    spacing: Kirigami.Units.smallSpacing / 2

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
        color: Kirigami.ColorUtils.tintWithAlpha(Kirigami.Theme.textColor, Kirigami.Theme.backgroundColor, 0.88)

        Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: parent.width * Math.max(0, Math.min(100, Number(row.percentLeft))) / 100
            radius: parent.radius
            color: row.accentColor
        }
    }
}
