// Tinted SVG icon — the QML equivalent of Slint's QbzIcon (glyphs recolored
// to a theme tint).
//
// POC-NOTE (icon tinting): the QBZ icon set is Feather-style SVGs with a
// hardcoded `stroke="#ffffff"`; Slint recolors at render time, QML's Image
// cannot. A ColorOverlay (Qt5Compat.GraphicalEffects) was tried first and
// renders NOTHING on the offscreen/software path (verified with a probe —
// effects need the GL path), so tinting is done with PRE-BAKED SVG
// variants: assets/icons/<tint>/<name>.svg for tint in {primary,
// secondary, muted, accent, warning} (script-generated from the originals,
// also rewriting fill="currentColor"). `tintName` switches variants
// dynamically (hover = secondary -> primary), served from qrc.
//
// TWO WAYS A BAKE SILENTLY LIES — both have already shipped here:
//
//  1. NO VARIANT ON DISK -> the Image resolves to nothing and the glyph is
//     simply ABSENT. Nothing is logged. Every `name` an
//     app site can ask for must exist in every `tintName` that site can ask
//     for; `favorite/` and `amber/` are deliberately partial (favorite =
//     heart/heart-filled/link/trash-2..., amber = monitor-speaker only), so
//     only widen a caller's tint set after checking the dir.
//  2. THE BAKE RAN BUT RECOLOURED NOTHING -> the glyph renders in the SVG's
//     OWN colour in every tint, which for a Feather-style file with no
//     paint attribute means BLACK (SVG's initial `fill` is black, and
//     `currentColor` with no inherited `color` resolves to black too). The
//     original baker only rewrote `stroke="..."` / `fill="currentColor"` on
//     the ROOT element, so three icons whose colour lives elsewhere came out
//     byte-identical in all seven dirs: element-connect (fill-based, no fill
//     attribute at all), home-gear (same, plus a `fill="none"` backdrop
//     path that must stay none) and copy (`stroke="currentColor"`).
//     Re-baked by putting the tint on the ROOT element and letting the
//     children inherit it — the convention play-fill/qbz-symbolic already
//     used. CHECK FOR THIS with a content hash: if
//     primary/<name>.svg == black/<name>.svg the icon is not tinted at all.

import QtQuick

Image {
    id: root
    // Icon file name, e.g. "compass" (".svg" appended).
    property string name: ""
    // One of: "primary" | "secondary" | "muted" | "accent" | "warning" |
    // "black" (for glyphs on white discs).
    property string tintName: "secondary"

    // Absolute qrc URL: relative URLs in a shared component resolve
    // against the CONSUMER's document depth (phase-23 dir layout — root
    // consumers underflowed one level).
    source: name === "" ? "" : "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/icons/" + tintName + "/" + name + ".svg"
    sourceSize: Qt.size(Math.max(1, Math.round(width * 2)), Math.max(1, Math.round(height * 2)))
    fillMode: Image.PreserveAspectFit
    asynchronous: false
}
