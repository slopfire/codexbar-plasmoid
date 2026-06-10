import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

RowLayout {
    id: metric

    property string title: ""
    property string value: ""

    PlasmaComponents3.Label {
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
    }
}
