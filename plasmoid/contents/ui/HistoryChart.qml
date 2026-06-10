import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Item {
    id: chart

    property var points: []
    property color accentColor: Kirigami.Theme.highlightColor
    readonly property var values: points.map(function(point) {
        return Number.isFinite(Number(point.costUSD)) ? Number(point.costUSD) : Number(point.totalTokens || 0);
    })
    readonly property real maxValue: Math.max(0, ...values)

    RowLayout {
        anchors.fill: parent
        spacing: 2

        Repeater {
            model: chart.values

            Rectangle {
                required property real modelData

                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.alignment: Qt.AlignBottom
                color: "transparent"

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: chart.maxValue > 0 ? parent.height * Math.max(0.04, modelData / chart.maxValue) : 0
                    radius: Math.max(1, width / 2)
                    color: chart.accentColor
                    opacity: 0.85
                }
            }
        }
    }
}
