// RoundedImage — an Image with TRUE rounded corners (PreserveAspectCrop).
//
// Why this exists: QML's `clip` is rectangular — a Rectangle with
// `radius` + `clip: true` does NOT clip children to the rounded shape
// (proven with an isolated scene on this Qt build: the child paints
// square over the rounded fill). Shader masks (Qt5Compat OpacityMask,
// QtQuick.Effects MultiEffect) render NOTHING on the software/offscreen
// path (the phase-2 icon-tinting probe), and the Pi kiosk can run the
// software path — so rounding is done with a Canvas raster clip
// (QPainter, antialiased, renderTarget Image = CPU).
//
// Usage mirrors the old pattern: `RoundedImage { anchors.fill: parent;
// source: artPath; radius: theme.radiusSm }` inside the same rounded
// placeholder Rectangle (the placeholder's OWN fill rounds fine — only
// children needed the workaround).

import QtQuick

Canvas {
    id: root
    property string source: ""
    property real radius: 8
    // "crop" (PreserveAspectCrop — album art) | "contain"
    // (PreserveAspectFit — label logos, never cropped, LabelCard.slint).
    property string fit: "crop"

    renderTarget: Canvas.Image
    renderStrategy: Canvas.Cooperative

    // Dimension probe: Canvas can't query the loaded image's intrinsic
    // size, and PreserveAspectCrop needs it. A hidden Image doubles as
    // the async loader notification.
    Image {
        id: probe
        source: root.source
        visible: false
        asynchronous: true
        cache: true
        onStatusChanged: if (status === Image.Ready || status === Image.Error) root.requestPaint()
    }

    onSourceChanged: loadImage(source)
    Component.onCompleted: loadImage(source)
    onImageLoaded: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()

    onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        var w = width
        var h = height
        if (w <= 0 || h <= 0) return
        var r = Math.max(0, Math.min(radius, w / 2, h / 2))
        if (r > 0) {
            ctx.beginPath()
            ctx.moveTo(r, 0)
            ctx.lineTo(w - r, 0)
            ctx.arcTo(w, 0, w, r, r)
            ctx.lineTo(w, h - r)
            ctx.arcTo(w, h, w - r, h, r)
            ctx.lineTo(r, h)
            ctx.arcTo(0, h, 0, h - r, r)
            ctx.lineTo(0, r)
            ctx.arcTo(0, 0, r, 0, r)
            ctx.closePath()
            ctx.clip()
        }
        if (probe.status === Image.Ready && probe.sourceSize.width > 0 && isImageLoaded(source)) {
            // "crop" = uniform scale covering the rect; "contain" = fitting
            // inside it. Both centered.
            var iw = probe.sourceSize.width
            var ih = probe.sourceSize.height
            var scale = fit === "contain" ? Math.min(w / iw, h / ih) : Math.max(w / iw, h / ih)
            var dw = iw * scale
            var dh = ih * scale
            ctx.drawImage(source, (w - dw) / 2, (h - dh) / 2, dw, dh)
        }
    }
}
