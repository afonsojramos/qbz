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
//
// ── THE RASTER IS IN DEVICE PIXELS ─────────────────────────────────────────
// `onPaint` lays everything out in DEVICE pixels and pre-divides the context
// by the device pixel ratio, instead of drawing in the item's logical pixels.
//
// Measured on Qt 6.11 (200x200 item, 600x600 cover, xcb, QT_SCALE_FACTOR=2,
// error vs a Lanczos 600->400 reference): drawing in logical pixels lands at
// RMSE 0.100 and its content is bit-for-bit the DPR-1 200x200 raster blown up
// 2x (RMSE 0.0026 against exactly that upscale) — half the detail the screen
// can show. The same paint in device pixels lands at RMSE 0.061 with a real
// 400x400 raster. At DPR 1 `scale(1,1)` makes this byte-identical to the old
// body, so it costs nothing on an unscaled screen; the backing store is
// device-sized either way, so it costs no extra memory on a scaled one.
//
// `_dpr` is a property, not a local, so a window dragged from a 1x screen to
// a 2x one repaints instead of leaving the old raster stretched.
//
// ── WHAT THIS DOES **NOT** FIX (see also QbzSkeleton) ──────────────────────
// `Context2D.drawImage` does NOT filter when it scales: measured against
// ImageMagick references, a 600 -> 200 draw is EXACTLY nearest-neighbour
// (RMSE 0.000 vs -filter Point) and so is a mild 230 -> 200 draw. The Canvas
// is loss-FREE only at 1:1 (RMSE 0.000 with a 200px source). Qt gives no way
// to ask `Canvas.loadImage()` for a decode size — `sourceSize` on the probe
// does not leak into it (the QQuickPixmap cache keys on the requested size),
// `drawImage(<Image item>)` draws nothing, and `grabToImage` refuses a hidden
// item. The cure is therefore a SOURCE FILE already at the drawn device size,
// which has to come from the Rust artwork pipeline — see GLUE NEEDED in the
// handoff. Until it lands, cards fed a 600px cover are point-sampled.

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz

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

    /// ── THE SCALED SOURCE ──────────────────────────────────────────────────
    /// drawImage does not filter when it scales, so a 600px cover in a 200px
    /// cell is point-sampled — measured RMSE 0.000 against an ImageMagick
    /// `-filter Point` reference. The cure is a source file ALREADY at the
    /// drawn device size, so the draw is 1:1 and there is no resample at all.
    /// Rust produces the derivative once per (cover, size) and answers on
    /// artScaledReady, keyed by the REQUEST so a recycled delegate cannot take
    /// the previous row's cover.
    property string _scaled: ""
    readonly property string _effectiveSource: root._scaled !== "" ? root._scaled : root.source
    /// Device pixels the art is actually drawn at, rounded up so a fractional
    /// DPR never asks for less than the screen shows.
    readonly property int _reqW: Math.ceil(root.width * root._dpr)
    readonly property int _reqH: Math.ceil(root.height * root._dpr)

    function _requestScaled() {
        if (root.source === "" || root._reqW <= 0 || root._reqH <= 0) return
        if (typeof QbzSession === "undefined") return
        QbzSession.artScaled(root.source, root._reqW, root._reqH)
    }
    Connections {
        target: typeof QbzSession !== "undefined" ? QbzSession : null
        function onArtScaledReady(path, scaled) {
            // Keyed on the REQUEST: a recycled delegate that has already moved
            // on ignores the answer meant for its previous row.
            if (path === root.source && scaled !== "") root._scaled = scaled
        }
    }

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
        source: root._effectiveSource
        visible: false
        asynchronous: true
        cache: true
        onStatusChanged: if (status === Image.Ready || status === Image.Error) root.requestPaint()
    }

    // requestPaint() as well as loadImage(): a delegate recycled onto a row
    // with NO cover assigns source = "", which emits no imageLoaded, so
    // without an explicit repaint the canvas would keep showing the previous
    // row's art under a placeholder that thinks it has nothing to cover.
    // A new cover invalidates the derivative: showing the previous row's
    // scaled file while the new one resolves is the stale-content bug again.
    onSourceChanged: { root._scaled = ""; loadImage(root._effectiveSource); _requestScaled(); requestPaint() }
    on_EffectiveSourceChanged: { loadImage(root._effectiveSource); requestPaint() }
    Component.onCompleted: { loadImage(root._effectiveSource); _requestScaled() }
    onImageLoaded: requestPaint()
    onWidthChanged: { requestPaint(); _requestScaled() }
    onHeightChanged: { requestPaint(); _requestScaled() }

    /// Device pixels per logical pixel for the screen this item is on. Held as
    /// a property so moving the window between screens of different scales
    /// re-rasterizes (see the DEVICE PIXELS note above).
    readonly property real _dpr: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    on_DprChanged: { requestPaint(); _requestScaled() }

    onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        // Everything below is in DEVICE pixels; the context is pre-divided so
        // the item still occupies `width` x `height` logical pixels.
        var dpr = root._dpr
        var w = Math.round(width * dpr)
        var h = Math.round(height * dpr)
        if (w <= 0 || h <= 0) return
        if (dpr !== 1) ctx.scale(1 / dpr, 1 / dpr)
        var r = Math.max(0, Math.min(radius * dpr, w / 2, h / 2))
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
