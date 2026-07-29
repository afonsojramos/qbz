// Tinted SVG icon — the QML equivalent of Slint's QbzIcon (glyphs recolored
// to a theme tint).
//
// HOW TINTING WORKS HERE (and why it is not what Slint does)
//
// Slint's QbzIcon is `Image { colorize: <live Theme token> }`: the renderer
// recolours the glyph every frame, so an icon always matches the current
// theme, all 36 of them, custom accent included. Qt has no equivalent on the
// path this port runs on — ColorOverlay / MultiEffect / shaders draw NOTHING
// on the software renderer (verified with a probe during the POC), which is
// why this file used to point straight at PRE-BAKED variants,
// assets/icons/<tint>/<name>.svg, each with a fixed hex burned in.
//
// Fixed hexes cannot serve 36 themes: the bakes were cut against the dark
// palette, so `secondary` (#cccccc) and `muted` (#888888) were near-white
// glyphs on near-white surfaces on every LIGHT theme, and `accent` was
// #4285f4 whatever the theme's accent actually was.
//
// So the tinted set is now GENERATED AT RUNTIME, into the user cache dir, by
// src/icon_tint_qt.rs: `QbzIconTint.dirFor(tint, themeJson)` returns the
// file:// directory holding this tint's glyphs for the live theme, and the
// Image loads a real .svg off disk — byte for byte the same load path as the
// qrc bakes, no effects involved. Passing `QbzShell.themeJson` is what makes
// the binding re-resolve on a live theme switch (the same dependency trick as
// `QbzSession.tr(msgid, trRev)`), and it is the colour source at the same
// time.
//
// ---------------------------------------------------------------------------
// THE TINT VOCABULARY — read this before adding a call site
// ---------------------------------------------------------------------------
//
// THEME-FOLLOWING (runtime-tinted, correct under all 36 themes):
//   "textPrimary"    Theme.text-primary — max contrast on a THEME surface
//   "secondary"      Theme.text-secondary   (alias: "textSecondary")
//   "muted"          Theme.text-muted       (alias: "textMuted")
//   "disabled"       Theme.text-disabled    (alias: "textDisabled")
//   "accent"         Theme.accent
//   "accentText"     the glyph colour ON an accent fill
//   "warning"        Theme.warning
//   "favorite"       Theme.favorite
//
// FIXED (theme-independent by design, served from the qrc bakes):
//   "white"          a light glyph on a host that is dark under EVERY theme:
//                    artwork scrims (#a6000000 / #b3000000), gradient discs,
//                    the close button's #e81123 hover
//   "primary"        LEGACY ALIAS OF "white". It has always been a literal
//                    #ffffff and ~14 call sites depend on that; it is NOT
//                    Theme.text-primary. New code says "white" when it means
//                    white and "textPrimary" when it means the theme token.
//   "black"          a dark glyph on a host painted light under every theme
//   "amber"          #e0b341, the audio stamp's brand accent (one icon)
//
// Getting this pair wrong is how the bug this file fixes happened in reverse:
// resolving "primary" to Theme.text-primary would paint a DARK glyph on a
// dark scrim on light themes. Slint keeps the two apart explicitly
// (CircleAction.slint:70-74) and so does this.
//
// ---------------------------------------------------------------------------
// THE QRC BAKES REMAIN, AS THE FLOOR
// ---------------------------------------------------------------------------
//
// `dirFor` returns "" — and this file falls back to assets/icons/<dir>/ —
// whenever the tint is fixed, there is no writable cache dir, the bake
// failed, the theme document has not been published yet, or QbzIconTint is
// not registered at all (see the `typeof` guard: an unregistered singleton
// resolves LAZILY and would otherwise take every icon in the app down with
// it). It also falls back per-icon on Image.Error, so a name that exists in
// the qrc set but not in the runtime masters degrades to the old fixed colour
// instead of rendering nothing.
//
// THE ONE WAY A BAKE STILL SILENTLY LIES — worth keeping in mind when adding
// an icon: NO VARIANT ON DISK -> the Image resolves to nothing and the glyph
// is simply ABSENT, with nothing logged. The runtime sets are always complete
// (every master, every tint), so this now only applies to the qrc fallback
// dirs, where `favorite/` and `amber/` are deliberately partial. A new icon
// goes into assets/icons/primary/ AND into the MASTERS table in
// src/icon_tint_qt.rs; the qrc dirs stay whatever they are.

import QtQuick
import com.blitzfc.qbz

Image {
    id: root
    // Icon file name, e.g. "compass" (".svg" appended).
    property string name: ""
    // One of the names in the vocabulary above.
    property string tintName: "secondary"

    // The qrc directory this tint falls back to. Every tint name — including
    // the ones with no directory of their own ("white", "textPrimary",
    // "accentText" ...) — must land on a REAL dir, or the fallback renders
    // nothing at all.
    readonly property string qrcTint: {
        switch (root.tintName) {
        case "white":
        case "textPrimary":
        case "accentText":
            return "primary"
        case "textSecondary":
            return "secondary"
        case "textMuted":
        case "textDisabled":
        case "disabled":
            return "muted"
        default:
            return root.tintName
        }
    }

    // Absolute qrc URL: relative URLs in a shared component resolve
    // against the CONSUMER's document depth (phase-23 dir layout — root
    // consumers underflowed one level).
    readonly property string qrcDir:
        "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/icons/" + root.qrcTint

    // The runtime-tinted directory for this tint under the live theme, or ""
    // when the qrc bake must be used (see above).
    readonly property string tintDir: {
        // Dependency: re-resolve every icon when the theme is republished.
        const themeJson = QbzShell.themeJson
        // `typeof` on an unregistered type name does not throw (plain JS
        // global lookup), so a missing registration degrades to the bakes
        // instead of poisoning every source binding with `undefined`.
        if (typeof QbzIconTint === "undefined")
            return ""
        const dir = QbzIconTint.dirFor(root.tintName, themeJson)
        return (typeof dir === "string") ? dir : ""
    }

    // Set once, per icon, if the runtime set has no such glyph (a master the
    // table does not carry yet). Cleared whenever the inputs change so a
    // recycled delegate is never stuck on the fallback.
    property bool qrcFallback: false
    onNameChanged: root.qrcFallback = false
    onTintNameChanged: root.qrcFallback = false
    onTintDirChanged: root.qrcFallback = false

    readonly property string activeDir:
        (root.tintDir === "" || root.qrcFallback) ? root.qrcDir : root.tintDir

    source: root.name === "" ? "" : root.activeDir + "/" + root.name + ".svg"
    onStatusChanged: {
        if (status === Image.Error && !root.qrcFallback && root.tintDir !== "")
            root.qrcFallback = true
    }

    sourceSize: Qt.size(Math.max(1, Math.round(width * 2)), Math.max(1, Math.round(height * 2)))
    fillMode: Image.PreserveAspectFit
    asynchronous: false
}
