import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property alias cfg_compactMetric: compactMetric.currentValue
    property alias cfg_compactShowMetric: compactShowMetric.checked
    property alias cfg_compactDisplay: compactDisplay.currentValue
    property alias cfg_compactBarsProviders: compactBarsProviders.currentValue
    property alias cfg_compactBarsTint: compactBarsTint.currentValue
    property alias cfg_compactProviderBarWidth: compactProviderBarWidth.value

    // Preserve runtime keys when Apply rewrites the General group.
    property string cfg_selectedEntryIds: "[]"
    property string cfg_selectedProviders: "[]"
    property string cfg_compactBarCatalog: "{}"

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QtControls.ComboBox {
                id: compactMetric
                Kirigami.FormData.label: i18n("Compact metric:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Lowest remaining limit"), value: "lowest" },
                    { text: i18n("Session remaining"), value: "session" },
                    { text: i18n("Weekly remaining"), value: "weekly" },
                    { text: i18n("Credits remaining"), value: "credits" },
                    { text: i18n("Today cost"), value: "todayCost" }
                ]
            }

            QtControls.CheckBox {
                id: compactShowMetric
                text: i18n("Show metric text")
            }

            QtControls.ComboBox {
                id: compactDisplay
                Kirigami.FormData.label: i18n("Tray display:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Provider icon"), value: "icon" },
                    { text: i18n("Usage bars — first larger"), value: "bars-first" },
                    { text: i18n("Usage bars — equal"), value: "bars-equal" },
                    { text: i18n("Usage bars — descending"), value: "bars-descending" },
                    { text: i18n("Usage bars — last larger"), value: "bars-last" },
                    { text: i18n("Usage bars — alternating"), value: "bars-alternating" }
                ]
            }

            QtControls.ComboBox {
                id: compactBarsProviders
                Kirigami.FormData.label: i18n("Tray usage bars:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Default provider"), value: "default" },
                    { text: i18n("Selected providers"), value: "selected" },
                    { text: i18n("All providers"), value: "all" }
                ]
            }

            QtControls.ComboBox {
                id: compactBarsTint
                Kirigami.FormData.label: i18n("Bar tint:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Provider colors"), value: "provider" },
                    { text: i18n("Remaining limit (white→yellow→red)"), value: "threshold" },
                    { text: i18n("Pace to reset (white→yellow→red)"), value: "pace" },
                    { text: i18n("Theme text"), value: "theme" }
                ]
            }

            QtControls.SpinBox {
                id: compactProviderBarWidth
                Kirigami.FormData.label: i18n("Provider bar width:")
                from: 8
                to: 96
                stepSize: 2
                editable: true
                textFromValue: function(value) {
                    return i18n("%1 px", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 18;
                }
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("Per-provider tray bars, colors, and “show in all-provider tray” options live on the Providers page.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }
}
