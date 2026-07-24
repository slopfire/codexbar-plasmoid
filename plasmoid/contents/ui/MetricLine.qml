import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

RowLayout {
    id: metric

    property string title: ""
    property string value: ""
    // When > 0, reserve a fixed title column so stacked MetricLines align values.
    property real titleWidth: 0
    property int valueAlignment: Text.AlignLeft

    spacing: Kirigami.Units.smallSpacing

    PlasmaComponents3.Label {
        id: titleLabel
        Layout.preferredWidth: metric.titleWidth > 0 ? metric.titleWidth : implicitWidth
        Layout.maximumWidth: metric.titleWidth > 0 ? metric.titleWidth : -1
        text: metric.title
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        elide: Text.ElideRight
    }

    PlasmaComponents3.Label {
        Layout.fillWidth: true
        text: metric.value
        font: Kirigami.Theme.smallFont
        elide: Text.ElideRight
        horizontalAlignment: metric.valueAlignment
    }
}
