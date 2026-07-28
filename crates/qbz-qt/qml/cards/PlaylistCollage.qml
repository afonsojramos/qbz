// PlaylistCollage — the playlist cover mosaic (discover/PlaylistCollage.slint,
// itself the port of Tauri's PlaylistCollage.svelte).
//
// Qobuz hands back up to four MEMBER-ALBUM covers per playlist
// (images300 / images150 / images); the layout is picked by how many there
// are — single, dual (side by side), triple (one large left + two stacked
// right), quad (three stacked left + one large right), and a list-music
// glyph when the playlist has none. Tiles are always crop-fitted; the
// playlist's OWN Qobuz graphic is NOT drawn here (PlaylistCard renders that
// one contain-fitted through RoundedImage).
//
// Canvas rather than a Grid of Images for the reason theme/RoundedImage.qml
// documents: QML `clip` is rectangular, so a Rectangle with `radius` does
// NOT round its children, and shader masks render nothing on the
// software/offscreen path. One Canvas per card, CPU raster, same cost
// profile as the RoundedImage every AlbumCard already mounts.
//
// Artwork is URL-keyed, not artKey-keyed (a playlist row has ONE artKey but
// up to four covers): the component hands its urls to the shared url-keyed
// resolver — `QbzShell.sidebarArtworkWindow`, the same entry point the
// sidebar tree's micro-collage uses — and listens for the
// `libraryArtworkReady` echo. Disk hits come back synchronously; misses are
// downloaded once and cached. The dispatch is debounced so flinging through
// a windowed grid never fetches covers for delegates that are already gone.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    // Remote cover urls (feed row `covers`); only the first four distinct
    // entries are drawn.
    property var urls: []
    property real radius: 8
    // Mosaic seam, 1:1 with PlaylistCollage.slint.
    property real gap: 2

    QbzTheme { id: theme }

    // De-duplicated + capped at four (Tauri dedupes the same way: repeated
    // albums in a playlist must not paint the same tile twice).
    readonly property var tiles: root.dedup(root.urls)
    // url -> local cached path, filled by the artwork signal.
    property var pathMap: ({})

    function dedup(list) {
        var out = []
        var seen = ({})
        var src = list || []
        for (var i = 0; i < src.length && out.length < 4; i++) {
            var u = src[i]
            if (!u || seen[u] === true)
                continue
            seen[u] = true
            out.push(u)
        }
        return out
    }

    function pathAt(index) {
        var u = root.tiles[index]
        return u ? (root.pathMap[u] || "") : ""
    }

    // Placeholder surface. A Rectangle's OWN rounded fill paints correctly —
    // only its children need the Canvas clip.
    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: theme.surfaceElevated
    }

    // No covers at all — the list-music glyph (PlaylistCollage.slint n <= 0).
    QbzIcon {
        visible: root.tiles.length === 0
        name: "list-music"
        width: Math.round(root.width * 0.32)
        height: width
        anchors.centerIn: parent
        tintName: "muted"
    }

    // Debounced url dispatch: one call per settled delegate, only for tiles
    // whose path is still unknown.
    Timer {
        id: dispatchDelay
        interval: 120
        onTriggered: {
            var pending = []
            for (var i = 0; i < root.tiles.length; i++) {
                var u = root.tiles[i]
                if (!root.pathMap[u])
                    pending.push(u)
            }
            if (pending.length > 0)
                QbzShell.sidebarArtworkWindow(JSON.stringify(pending))
        }
    }

    onTilesChanged: {
        if (root.tiles.length > 0)
            dispatchDelay.restart()
    }
    Component.onCompleted: {
        if (root.tiles.length > 0)
            dispatchDelay.restart()
    }

    Connections {
        target: QbzLibrary
        // Shared with the feed's artKey-keyed emissions — ignore anything
        // that is not one of OUR urls.
        function onLibraryArtworkReady(key, path) {
            if (root.pathMap[key] === path)
                return
            var mine = false
            for (var i = 0; i < root.tiles.length; i++) {
                if (root.tiles[i] === key) {
                    mine = true
                    break
                }
            }
            if (!mine)
                return
            var m = root.pathMap
            m[key] = path
            // Rebind requires a NEW object reference.
            root.pathMap = Object.assign({}, m)
        }
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        renderTarget: Canvas.Image
        // See RoundedImage.qml: GUI-thread raster. This canvas repaints from
        // libraryArtworkReady and from four async image probes, on a card the
        // feed recycles.
        renderStrategy: Canvas.Immediate

        // Dimension probes: Canvas cannot query a loaded image's intrinsic
        // size and the crop fit needs it (RoundedImage's pattern, one per
        // tile). They double as the async-load notification.
        Image {
            id: p0
            source: root.pathAt(0)
            visible: false
            asynchronous: true
            cache: true
            onSourceChanged: canvas.prime(source)
            onStatusChanged: canvas.requestPaint()
        }
        Image {
            id: p1
            source: root.pathAt(1)
            visible: false
            asynchronous: true
            cache: true
            onSourceChanged: canvas.prime(source)
            onStatusChanged: canvas.requestPaint()
        }
        Image {
            id: p2
            source: root.pathAt(2)
            visible: false
            asynchronous: true
            cache: true
            onSourceChanged: canvas.prime(source)
            onStatusChanged: canvas.requestPaint()
        }
        Image {
            id: p3
            source: root.pathAt(3)
            visible: false
            asynchronous: true
            cache: true
            onSourceChanged: canvas.prime(source)
            onStatusChanged: canvas.requestPaint()
        }

        function prime(src) {
            if (src)
                canvas.loadImage(src)
            canvas.requestPaint()
        }

        function probeAt(index) {
            return index === 0 ? p0 : index === 1 ? p1 : index === 2 ? p2 : p3
        }

        // One crop-fitted tile, clipped to its own rect (uniform scale
        // covering the cell, centered — Image.PreserveAspectCrop).
        function tile(ctx, index, x, y, tw, th) {
            var src = root.pathAt(index)
            if (!src || tw <= 0 || th <= 0)
                return
            var probe = canvas.probeAt(index)
            if (probe.status !== Image.Ready || probe.sourceSize.width <= 0
                    || probe.sourceSize.height <= 0)
                return
            if (!canvas.isImageLoaded(src)) {
                canvas.loadImage(src)
                return
            }
            var iw = probe.sourceSize.width
            var ih = probe.sourceSize.height
            var scale = Math.max(tw / iw, th / ih)
            var dw = iw * scale
            var dh = ih * scale
            ctx.save()
            ctx.beginPath()
            ctx.rect(x, y, tw, th)
            ctx.clip()
            ctx.drawImage(src, x + (tw - dw) / 2, y + (th - dh) / 2, dw, dh)
            ctx.restore()
        }

        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onImageLoaded: requestPaint()

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            var w = width
            var h = height
            if (w <= 0 || h <= 0)
                return
            var n = root.tiles.length
            if (n <= 0)
                return

            var r = Math.max(0, Math.min(root.radius, w / 2, h / 2))
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

            var g = root.gap
            if (n === 1) {
                canvas.tile(ctx, 0, 0, 0, w, h)
            } else if (n === 2) {
                // Side-by-side halves.
                var half = (w - g) / 2
                canvas.tile(ctx, 0, 0, 0, half, h)
                canvas.tile(ctx, 1, half + g, 0, half, h)
            } else if (n === 3) {
                // One large left (62%), two stacked right (38%).
                var big3 = (w - g) * 0.62
                var small3 = (w - g) * 0.38
                var half3 = (h - g) / 2
                canvas.tile(ctx, 0, 0, 0, big3, h)
                canvas.tile(ctx, 1, big3 + g, 0, small3, half3)
                canvas.tile(ctx, 2, big3 + g, half3 + g, small3, half3)
            } else {
                // Three stacked left (34%), one large right (66%).
                var col4 = (w - g) * 0.34
                var big4 = (w - g) * 0.66
                var row4 = (h - 2 * g) / 3
                canvas.tile(ctx, 0, 0, 0, col4, row4)
                canvas.tile(ctx, 1, 0, row4 + g, col4, row4)
                canvas.tile(ctx, 2, 0, 2 * (row4 + g), col4, row4)
                canvas.tile(ctx, 3, col4 + g, 0, big4, h)
            }
        }
    }
}
