import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3

RowLayout {
    id: badge

    property var status
    readonly property string indicator: status && status.indicator ? status.indicator : ""
    visible: indicator.length > 0
    spacing: Kirigami.Units.smallSpacing / 2

    Rectangle {
        Layout.preferredWidth: Kirigami.Units.smallSpacing
        Layout.preferredHeight: Kirigami.Units.smallSpacing
        radius: width / 2
        color: statusColor()
    }

    PlasmaComponents3.Label {
        text: statusText()
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        elide: Text.ElideRight
    }

    function statusText() {
        if (!status) {
            return "";
        }
        if (status.description) {
            return status.description;
        }
        switch (indicator) {
        case "none":
            return i18n("Operational");
        case "minor":
            return i18n("Partial outage");
        case "major":
            return i18n("Major outage");
        case "critical":
            return i18n("Critical issue");
        case "maintenance":
            return i18n("Maintenance");
        default:
            return i18n("Status unknown");
        }
    }

    function statusColor() {
        switch (indicator) {
        case "none":
            return Kirigami.Theme.positiveTextColor;
        case "minor":
        case "maintenance":
            return Kirigami.Theme.neutralTextColor;
        case "major":
        case "critical":
            return Kirigami.Theme.negativeTextColor;
        default:
            return Kirigami.Theme.disabledTextColor;
        }
    }
}
