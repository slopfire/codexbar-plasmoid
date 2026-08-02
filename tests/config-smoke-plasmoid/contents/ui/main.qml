import QtQuick
import org.kde.plasma.plasmoid
import org.kde.plasma.plasma5support as Plasma5Support

PlasmoidItem {
    id: harness

    width: 900
    height: 700

    property int attempts: 0
    property bool resultRecorded: false

    Plasma5Support.DataSource {
        id: resultMarker
        engine: "executable"
        connectedSources: []
        onNewData: function(sourceName) { disconnectSource(sourceName); }
    }

    Loader {
        id: providersLoader
        anchors.fill: parent
        source: "../../../../plasmoid/contents/ui/configProviders.qml"

        onStatusChanged: {
            if (status === Loader.Error) {
                console.error("CONFIG_SMOKE_FAIL loader error");
                Qt.quit();
            }
        }
    }

    Timer {
        interval: 100
        repeat: true
        running: !harness.resultRecorded
        onTriggered: {
            harness.attempts += 1;
            const page = providersLoader.item;
            if (!page || page.accountDiscoveryBusy) {
                if (harness.attempts < 150) {
                    return;
                }
                console.error("CONFIG_SMOKE_FAIL account discovery timeout");
                Qt.quit();
                return;
            }

            const choices = page.accountChoices("codex", "", 0);
            let foundActive = false;
            let foundDetected = false;
            let foundAll = false;
            for (let index = 0; index < choices.length; index += 1) {
                foundActive = foundActive || choices[index].kind === "active";
                foundDetected = foundDetected
                    || String(choices[index].value) === "mock-codex@example.com";
                foundAll = foundAll || choices[index].kind === "all";
            }
            if (foundActive && foundDetected && foundAll) {
                harness.resultRecorded = true;
                resultMarker.connectSource("/usr/bin/touch /tmp/codexbar-config-smoke.pass");
                return;
            }
            if (harness.attempts >= 150) {
                console.error(
                    "CONFIG_SMOKE_FAIL choices=" + JSON.stringify(choices)
                    + " status=" + String(page.accountDiscoveryStatus)
                );
                Qt.quit();
            }
        }
    }
}
