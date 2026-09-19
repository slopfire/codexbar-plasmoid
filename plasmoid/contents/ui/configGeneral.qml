import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property alias cfg_cliPath: cliPath.text
    property alias cfg_autoUpdateCli: autoUpdateCli.checked
    property string cfg_cliUpdateChannel: cliUpdateChannel.currentValue
    property alias cfg_refreshIntervalSeconds: refreshInterval.value
    property alias cfg_costRefreshIntervalSeconds: costRefreshInterval.value
    property alias cfg_shareProviderFetches: shareProviderFetches.checked
    property alias cfg_requestTimeoutSeconds: requestTimeout.value

    // Preserve runtime keys when Apply rewrites the General group.
    property string cfg_selectedEntryIds: "[]"
    property string cfg_selectedProviders: "[]"
    property string cfg_compactBarCatalog: "{}"

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QtControls.TextField {
                id: cliPath
                Kirigami.FormData.label: i18n("CLI executable:")
                placeholderText: "codexbar"
            }

            QtControls.CheckBox {
                id: autoUpdateCli
                Kirigami.FormData.label: i18n("CLI updates:")
                text: i18n("Auto-download from GitHub")
            }

            QtControls.ComboBox {
                id: cliUpdateChannel
                Kirigami.FormData.label: i18n("Release channel:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Latest stable"), value: "latest" }
                ]
                currentIndex: 0
                enabled: cfg_autoUpdateCli
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: cfg_autoUpdateCli
                    ? i18n("Managed binary will be installed at ~/.local/share/codexbar-plasmoid/bin/codexbar")
                    : i18n("Leave the executable as \"codexbar\" to use the binary on PATH")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }

            QtControls.SpinBox {
                id: refreshInterval
                Kirigami.FormData.label: i18n("Usage refresh (seconds):")
                from: 60
                to: 86400
                stepSize: 30
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 second", "%1 seconds", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 150;
                }
            }

            QtControls.SpinBox {
                id: costRefreshInterval
                Kirigami.FormData.label: i18n("Cost history refresh (minutes):")
                from: 300
                to: 604800
                stepSize: 300
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 minute", "%1 minutes", Math.round(value / 60));
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed * 60 : 3600;
                }
            }

            QtControls.SpinBox {
                id: requestTimeout
                Kirigami.FormData.label: i18n("Timeout:")
                from: 5
                to: 300
                stepSize: 5
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 second", "%1 seconds", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 45;
                }
            }

            QtControls.CheckBox {
                id: shareProviderFetches
                Kirigami.FormData.label: i18n("Multiple widgets:")
                text: i18n("Share matching provider refreshes")
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("Widgets using the same CLI and provider settings reuse usage results. Cost history uses its own refresh interval.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }
}
