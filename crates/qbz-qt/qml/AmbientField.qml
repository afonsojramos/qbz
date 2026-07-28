// AmbientField — the app-wide dynamic background ("modo Cider", phase 14).
//
// The QML rendering of the Slint ambient WGSL scene (crates/qbz-ui/ui/
// shaders/ambient.wgsl): FOUR album-colored blobs on the shader's EXACT
// wandering orbits (10-18s, t scaled 0.75), additive-blended over a black
// base so near blobs fuse like the metaball field, plus the shader's
// vertical edge shade. Colors come from the ambient_qt album triad
// (QbzBridge.ambientPrimary/Secondary/Accent, recomputed on track change —
// the field crossfades because Canvas repaints are continuous, ~0.6s of
// visible blend per the blob softness).
//
// POC-NOTEs (vs the WGSL original, ambient_qt.rs has the full list):
// - No fbm domain warp and no r²/d² metaball potential — plain radial
//   gradients with 'lighter' compositing approximate the fusion.
// - No audio breathe (the POC has no FFT tap; level_smooth = 0 → the
//   shader's breathe is a constant 1.0 there anyway) and no grain dither.
// - Rendered with QPainter (renderTarget Image) so it works on the
//   software/offscreen path too; 30fps cap + paused when the window is
//   inactive (the spec's cost controls §6).

import QtQuick
import com.blitzfc.qbz

Canvas {
    id: root

    // The album triad (hex strings from ambient_qt).
    property color primary: QbzBridge.ambientPrimary
    property color secondary: QbzBridge.ambientSecondary
    property color accent: QbzBridge.ambientAccent
    // Set false by the host to freeze the clock (inactive / hidden).
    property bool running: true

    renderTarget: Canvas.Image
    renderStrategy: Canvas.Cooperative

    // The shader clock (t = time * 0.75). Accumulated at ~30fps.
    property real t: Math.random() * 40.0

    Timer {
        interval: 33
        repeat: true
        running: root.running && root.visible && root.Window.active
        onTriggered: {
            root.t += interval / 1000.0 * 0.75
            root.requestPaint()
        }
    }

    onPrimaryChanged: requestPaint()
    onSecondaryChanged: requestPaint()
    onAccentChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()

    // One blob: radial gradient, center color at `a` alpha fading to
    // transparent (the shader's wide smoothstep falloff).
    function blob(ctx, cx, cy, r, c, a) {
        var g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r)
        g.addColorStop(0.0, Qt.rgba(c.r, c.g, c.b, a))
        g.addColorStop(0.55, Qt.rgba(c.r, c.g, c.b, a * 0.45))
        g.addColorStop(1.0, Qt.rgba(c.r, c.g, c.b, 0.0))
        ctx.fillStyle = g
        ctx.fillRect(cx - r, cy - r, 2 * r, 2 * r)
    }

    onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        var W = width
        var H = height
        if (W <= 0 || H <= 0) return

        // Base: the shader's field->0 color (black).
        ctx.fillStyle = "#000000"
        ctx.fillRect(0, 0, W, H)

        var t = root.t
        // The shader's orbit constants (ambient.wgsl cA..cD, aspect folded
        // back out — window-fraction centers are the plain expressions).
        var cA = [0.32 + 0.30 * Math.sin(t * 0.40), 0.42 + 0.30 * Math.cos(t * 0.33)]
        var cB = [0.66 + 0.30 * Math.sin(t * 0.35 + 2.1), 0.56 + 0.32 * Math.cos(t * 0.29 + 1.3)]
        var cC = [0.50 + 0.34 * Math.cos(t * 0.31 + 4.0), 0.36 + 0.30 * Math.sin(t * 0.45 + 3.2)]
        var cD = [0.46 + 0.32 * Math.sin(t * 0.27 + 5.3), 0.64 + 0.28 * Math.cos(t * 0.49 + 0.7)]

        // rr = 0.34 of the field; the shader's brightness lands ~0.92 with
        // the 0.42-1.12 shape spread — the alpha spread below reproduces
        // that mid-to-low brightness read under the host's dim scrim.
        var rr = 0.36 * Math.max(W, H)
        var c4 = Qt.rgba(
            (primary.r + accent.r) / 2, (primary.g + accent.g) / 2, (primary.b + accent.b) / 2, 1.0)

        ctx.globalCompositeOperation = "lighter"
        blob(ctx, cA[0] * W, cA[1] * H, rr, primary, 0.50)
        blob(ctx, cB[0] * W, cB[1] * H, rr * 0.95, secondary, 0.46)
        blob(ctx, cC[0] * W, cC[1] * H, rr * 0.88, accent, 0.44)
        blob(ctx, cD[0] * W, cD[1] * H, rr * 0.82, c4, 0.42)
        ctx.globalCompositeOperation = "source-over"

        // Vertical shade: 16% darker at the very top/bottom edges (the
        // shader's vshade — chrome sits on calmer color).
        var v = ctx.createLinearGradient(0, 0, 0, H)
        v.addColorStop(0.0, Qt.rgba(0, 0, 0, 0.16))
        v.addColorStop(0.5, Qt.rgba(0, 0, 0, 0.0))
        v.addColorStop(1.0, Qt.rgba(0, 0, 0, 0.16))
        ctx.fillStyle = v
        ctx.fillRect(0, 0, W, H)
    }
}
