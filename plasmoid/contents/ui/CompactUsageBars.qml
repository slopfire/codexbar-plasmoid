import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: bars

    property var usageRows: []
    property var barItems: []
    property color accentColor: Kirigami.Theme.highlightColor
    property bool stale: false

    readonly property var barsModel: normalizedBars()
    readonly property int barCount: Math.max(1, barsModel.length)
    readonly property real groupGap: barsModel.length > 1 ? 3 : 0
    readonly property real laneGap: 2

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium

    function normalizedBars() {
        const output = [];
        const items = bars.barItems || [];
        for (let index = 0; index < items.length; index += 1) {
            const item = items[index];
            const rows = [];
            const itemRows = item.rows || [];
            const maxRows = Math.min(itemRows.length, 2);
            for (let rowIndex = 0; rowIndex < maxRows; rowIndex += 1) {
                const row = itemRows[rowIndex];
                const rowValue = Number(row.percentLeft);
                if (!Number.isFinite(rowValue)) {
                    continue;
                }
                rows.push({
                    title: String(row.title || ""),
                    percentLeft: Math.max(0, Math.min(100, rowValue)),
                    color: row.color || item.color || bars.accentColor
                });
            }
            if (rows.length > 0) {
                output.push({
                    title: String(item.title || ""),
                    rows
                });
                continue;
            }

            const value = Number(item.percentLeft);
            if (!Number.isFinite(value)) {
                continue;
            }
            output.push({
                title: String(item.title || ""),
                rows: [{
                    title: String(item.title || ""),
                    percentLeft: Math.max(0, Math.min(100, value)),
                    color: item.color || bars.accentColor
                }]
            });
        }
        if (output.length > 0) {
            return output;
        }

        const rows = bars.usageRows || [];
        const fallbackRows = [];
        const maxRows = Math.min(rows.length, 2);
        for (let index = 0; index < maxRows; index += 1) {
            const row = rows[index];
            const value = Number(row.percentLeft);
            fallbackRows.push({
                title: String(row.title || ""),
                percentLeft: Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0,
                color: bars.accentColor
            });
        }
        if (fallbackRows.length > 0) {
            output.push({
                title: "",
                rows: fallbackRows
            });
        }
        return output;
    }

    readonly property color trackColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.18 : 0.28)
    readonly property color strokeColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.28 : 0.44)

    Repeater {
        model: bars.barsModel

        Item {
            readonly property var groupRows: modelData.rows || []

            y: 2
            width: Math.max(3, (bars.width - 4 - (bars.barCount - 1) * bars.groupGap) / bars.barCount)
            height: bars.height - 4
            x: 2 + index * (width + bars.groupGap)

            Repeater {
                model: groupRows

                Rectangle {
                    readonly property int laneCount: Math.max(1, groupRows.length)
                    readonly property real availableHeight: Math.max(0, parent.height - (laneCount - 1) * bars.laneGap)

                    x: 0
                    y: index * (height + bars.laneGap)
                    width: parent.width
                    height: Math.max(3, availableHeight / laneCount)
                    radius: height / 2
                    color: bars.trackColor
                    border.width: 1
                    border.color: bars.strokeColor

                    Rectangle {
                        anchors.left: parent.left
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        width: parent.width * Number(modelData.percentLeft) / 100
                        radius: parent.radius
                        color: bars.stale
                            ? Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.55)
                            : modelData.color
                    }
                }
            }
        }
    }
}
