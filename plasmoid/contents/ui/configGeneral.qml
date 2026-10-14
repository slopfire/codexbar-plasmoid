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
    property alias cfg_providerRetryAttempts: providerRetryAttempts.value
    property alias cfg_failureCooldownSeconds: failureCooldownSeconds.value
    property alias cfg_costScanTimeoutSeconds: costScanTimeoutSeconds.value
    property alias cfg_syncServerUrl: syncServerUrl.text
    property alias cfg_syncServerToken: syncServerToken.text
    property alias cfg_syncDeviceName: syncDeviceName.text

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

            QtControls.Label {
                Kirigami.FormData.isSection: true
                text: i18n("Reliability")
            }

            QtControls.SpinBox {
                id: providerRetryAttempts
                Kirigami.FormData.label: i18n("Retries per refresh:")
                from: 1
                to: 4
                stepSize: 1
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 attempt", "%1 attempts", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 2;
                }
            }

            QtControls.SpinBox {
                id: failureCooldownSeconds
                Kirigami.FormData.label: i18n("Failure backoff cap (seconds):")
                from: 30
                to: 3600
                stepSize: 30
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 second", "%1 seconds", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 180;
                }
            }

            QtControls.SpinBox {
                id: costScanTimeoutSeconds
                Kirigami.FormData.label: i18n("Cost scan timeout (seconds):")
                from: 60
                to: 1800
                stepSize: 30
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 second", "%1 seconds", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 300;
                }
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("A provider call that fails with a transient error (a busy socket, a rate limit) is retried within the same refresh. After that, the widget keeps showing the last good data and waits 30 seconds, doubling up to the backoff cap, before asking again. The cost scan runs in the background so it never delays the popup; raise its timeout for very large Codex histories.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }

            QtControls.TextField {
                id: syncServerUrl
                Kirigami.FormData.label: i18n("Sync server:")
                placeholderText: "http://192.168.1.10:8188"
            }

            QtControls.TextField {
                id: syncServerToken
                Kirigami.FormData.label: i18n("Sync token:")
                echoMode: TextInput.Password
                placeholderText: i18n("Bearer token from the server")
            }

            QtControls.TextField {
                id: syncDeviceName
                Kirigami.FormData.label: i18n("Device name:")
                placeholderText: "laptop"
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("When set, this widget publishes locally fetched usage to the sync server (codexbar-plasmoid sync-server) after each refresh. Provider rows set to the remote source read the merged cross-device data instead of fetching here. Credentials stay on the device with them; the anonymize option above applies to published data.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }
}
