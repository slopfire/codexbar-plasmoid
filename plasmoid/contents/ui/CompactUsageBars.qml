import QtQuick
import org.kde.kirigami as Kirigami

Item {
    id: bars

    property var usageRows: []
    property var barItems: []
    property color accentColor: Kirigami.Theme.highlightColor
    property bool stale: false
    property real barGroupWidth: 18

    readonly property var barsModel: normalizedBars()
    readonly property real groupGap: barsModel.length > 1 ? Kirigami.Units.smallSpacing : 0
    readonly property real laneGap: 2
    readonly property real defaultGroupWidth: Math.max(8, Number(bars.barGroupWidth || 18))

    function groupWidthFor(rowIndex) {
        const item = bars.barsModel[rowIndex];
        if (!item) {
            return bars.defaultGroupWidth;
        }
        const rows = item.rows || [];
        if (rows.length > 0 && rows[0].kind === "credits") {
            const text = String(rows[0].valueText || "");
            return Math.min(64, Math.max(bars.defaultGroupWidth, text.length * 6 + 4));
        }
        return bars.defaultGroupWidth;
    }

    function groupXFor(rowIndex) {
        let x = 0;
        for (let i = 0; i < rowIndex; i += 1) {
            x += bars.groupWidthFor(i) + bars.groupGap;
        }
        return x;
    }


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
                if (row.kind === "credits") {
                    rows.push({
                        kind: "credits",
                        title: String(row.title || ""),
                        valueText: String(row.valueText || ""),
                        color: row.color || item.color || bars.accentColor
                    });
                    continue;
                }
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
        bars.stale ? 0.30 : 0.45)
    readonly property color strokeColor: Qt.rgba(
        Kirigami.Theme.textColor.r,
        Kirigami.Theme.textColor.g,
        Kirigami.Theme.textColor.b,
        bars.stale ? 0.45 : 0.65)

    function textColorForBg(bg) {
        const lum = 0.299 * bg.r + 0.587 * bg.g + 0.114 * bg.b;
        return lum > 0.55 ? "#1a1a1a" : "white";
    }

    Repeater {
        model: bars.barsModel

        Item {
            readonly property var groupRows: modelData.rows || []

            y: 2
            width: bars.groupWidthFor(index)
            height: Math.max(0, bars.height - 5)
            x: bars.groupXFor(index)

            Repeater {
                model: groupRows

                Rectangle {
                    visible: modelData.kind !== "credits"
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
                        width: Math.max(3, parent.width * Number(modelData.percentLeft) / 100)
                        radius: parent.radius
                        color: bars.stale
                            ? Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.55)
                            : modelData.color
                    }
                }
            }

            Rectangle {
                visible: groupRows.length > 0 && groupRows[0].kind === "credits"
                anchors.fill: parent
                radius: Math.max(3, Math.min(height, parent.width) / 4)
                color: bars.stale
                    ? Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.55)
                    : groupRows[0].color
                border.width: 1
                border.color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.30)

                Text {
                    anchors.fill: parent
                    anchors.leftMargin: 2
                    anchors.rightMargin: 2
                    readonly property string _firstRowValueText:
                        groupRows.length > 0 && groupRows[0] && groupRows[0].kind === "credits"
                            ? String(groupRows[0].valueText || "")
                            : ""
                    text: _firstRowValueText
                    color: textColorForBg(parent.color)
                    font.bold: true
                    font.pixelSize: Math.max(7, Math.min(11, Math.floor(parent.height * 0.5)))
                    elide: Text.ElideRight
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }
}
