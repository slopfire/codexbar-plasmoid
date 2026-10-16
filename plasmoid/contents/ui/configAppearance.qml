import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property alias cfg_includeStatus: includeStatus.checked
    property alias cfg_includeCost: includeCost.checked
    property alias cfg_showCredits: showCredits.checked
    property alias cfg_showHistory: showHistory.checked
    property alias cfg_showTopBar: showTopBar.checked
    property alias cfg_showScrollbar: showScrollbar.checked
    property alias cfg_anonymizeEmail: anonymizeEmail.checked
    property alias cfg_paceIndicatorStyle: paceIndicatorStyle.currentValue

    // Preserve runtime keys when Apply rewrites the General group.
    property string cfg_selectedEntryIds: "[]"
    property string cfg_selectedProviders: "[]"
    property string cfg_compactBarCatalog: "{}"
    property bool cfg_syncAllFromServer: false

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QtControls.CheckBox {
                id: includeStatus
                Kirigami.FormData.label: i18n("Data:")
                text: i18n("Provider status")
            }

            QtControls.CheckBox {
                id: includeCost
                text: i18n("Local token costs (global)")
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("Per-provider Token & API costs checkboxes on the Providers page further filter which providers fetch spend stats.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
                visible: includeCost.checked
            }

            QtControls.CheckBox {
                id: showCredits
                Kirigami.FormData.label: i18n("Expanded view:")
                text: i18n("Credits")
            }

            QtControls.CheckBox {
                id: showHistory
                text: i18n("History chart")
            }

            QtControls.CheckBox {
                id: showTopBar
                text: i18n("Top bar (title, refresh, configure)")
            }

            QtControls.CheckBox {
                id: showScrollbar
                text: i18n("List scrollbar")
            }

            QtControls.ComboBox {
                id: paceIndicatorStyle
                Kirigami.FormData.label: i18n("Pace indicator:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Position color"), value: "position" },
                    { text: i18n("Current bar color"), value: "bar" },
                    { text: i18n("Provider color"), value: "provider" },
                    { text: i18n("Theme text color"), value: "theme" },
                    { text: i18n("Gaps only"), value: "gaps" }
                ]
            }

            QtControls.CheckBox {
                id: anonymizeEmail
                Kirigami.FormData.label: i18n("Privacy:")
                text: i18n("Anonymize emails")
            }
        }
    }
}
