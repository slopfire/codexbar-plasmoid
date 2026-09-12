import QtQuick
import QtQuick.Window
import org.kde.kirigami as Kirigami

Item {
    id: icon

    property url source
    property color color: Kirigami.Theme.textColor
    readonly property real screenScale: Math.max(1, Screen.devicePixelRatio)

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium

    onSourceChanged: painter.reloadSource()
    onColorChanged: painter.requestPaint()
    onScreenScaleChanged: painter.requestPaint()

    Canvas {
        id: painter

        width: icon.width * icon.screenScale
        height: icon.height * icon.screenScale
        scale: 1 / icon.screenScale
        transformOrigin: Item.TopLeft

        function reloadSource() {
            if (String(icon.source).length > 0) {
                loadImage(icon.source);
            }
            requestPaint();
        }

        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onImageLoaded: requestPaint()

        onPaint: {
            const context = getContext("2d");
            context.clearRect(0, 0, width, height);
            if (String(icon.source).length === 0 || !isImageLoaded(icon.source)) {
                return;
            }
            context.save();
            context.drawImage(icon.source, 0, 0, width, height);
            context.globalCompositeOperation = "source-in";
            context.fillStyle = icon.color;
            context.fillRect(0, 0, width, height);
            context.restore();
        }
    }
}
