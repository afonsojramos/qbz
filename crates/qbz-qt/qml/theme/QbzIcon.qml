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
