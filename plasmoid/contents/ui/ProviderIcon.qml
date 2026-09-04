import QtQuick
import org.kde.kirigami as Kirigami

Canvas {
    id: icon

    property url source
    property color color: Kirigami.Theme.textColor

    implicitWidth: Kirigami.Units.iconSizes.smallMedium
    implicitHeight: Kirigami.Units.iconSizes.smallMedium

    onSourceChanged: {
        if (String(source).length > 0) {
            loadImage(source);
        }
        requestPaint();
    }
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onImageLoaded: requestPaint()

    onPaint: {
        const context = getContext("2d");
        context.clearRect(0, 0, width, height);
        if (String(source).length === 0 || !isImageLoaded(source)) {
            return;
        }
        context.save();
        context.drawImage(source, 0, 0, width, height);
        context.globalCompositeOperation = "source-in";
        context.fillStyle = color;
        context.fillRect(0, 0, width, height);
        context.restore();
    }
}
