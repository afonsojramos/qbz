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
//
// ── READINESS CONTRACT (`ready`) ───────────────────────────────────────────
// A host that draws a loading placeholder over this item MUST gate it on
// `ready`, never on "the path is non-empty".
//
// The path landing means NOTHING here. This is a Canvas: once `source` is
// assigned the probe still has to load the file through QQuickPixmap and then
// a paint pass has to rasterize it. A placeholder gated on the path therefore
// vanishes the instant the path arrives and leaves an EMPTY tile on screen
// until the canvas paints — the reported bug ("the skeleton disappears before
// the art is rendered").
//
// `ready` is true only when BOTH have happened for the CURRENT source:
//   1. the dimension probe reached Image.Ready (the pixmap is decoded), and
//   2. `onPaint` has run a pass that actually drew THAT source.
// It is derived from `_paintedSource`, so re-assigning `source` drops it back
// to false with no bookkeeping — a recycled list delegate cannot inherit the
// previous row's readiness.
//
// Hosts that cannot reach this item (the image lives inside AlbumCard,
// PlaylistCard, …) use the equivalent self-probing arm of the placeholder
// itself: `QbzSkeleton { coverSource: <path> }` — see QbzSkeleton.qml.

import QtQuick

Canvas {
    id: root
    property string source: ""
    property real radius: 8
    // "crop" (PreserveAspectCrop — album art) | "contain"
    // (PreserveAspectFit — label logos, never cropped, LabelCard.slint).
    property string fit: "crop"

    /// THE handover signal — see the READINESS CONTRACT above. True only once
    /// the current `source` is on screen, not merely known.
    readonly property bool ready: root._paintedSource !== ""
        && root._paintedSource === root.source
    /// The source of the last paint that actually drew pixels. Internal; it
    /// exists as a property (not a bool flag) so that `ready` invalidates
    /// itself on every source change.
    property string _paintedSource: ""

    renderTarget: Canvas.Image
    // GUI-thread raster. Cooperative/Threaded rasterize on the SCENE GRAPH
    // RENDER THREAD, and this item is created and destroyed by list recycling
    // while async artwork callbacks schedule repaints — the render thread can
    // be mid-draw on a pixmap whose Canvas is already gone. Immediate keeps
    // paint and destruction on one thread. renderTarget: Image already meant
    // CPU raster, so this costs no visual change.
    renderStrategy: Canvas.Immediate

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

    // requestPaint() as well as loadImage(): a delegate recycled onto a row
    // with NO cover assigns source = "", which emits no imageLoaded, so
    // without an explicit repaint the canvas would keep showing the previous
    // row's art under a placeholder that thinks it has nothing to cover.
    onSourceChanged: { loadImage(source); requestPaint() }
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
            // ONLY here: pixels for `source` are now on the canvas. Anything
            // earlier (the path arriving, the probe going Ready) would hand
            // the cell over to an empty tile. Assigning the same string twice
            // is a no-op in QML, so repeated paints cost no notifications.
            root._paintedSource = source
        }
    }
}
