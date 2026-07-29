// QbzCircleAction — the circular header action, BOTH arms of Slint
// primitives/CircleAction.slint. Consolidated in phase 22 from THREE copies
// (AlbumView.CircleBtn ≡ ArtistView.CircleBtn, PlaylistView.CircleBtn =
// superset with the `primary` + `btnEnabled` arms).
//
// Numbers, read off primitives/CircleAction.slint:
//   :24  diameter — primary 44, secondary 32 (the hierarchy lives HERE, not
//        in the callers; the port had 40 and was uniformly small)
//   :66  glyph — primary 19, secondary 15
//   :48  ON-SURFACE arm (`overlay: false`, the default): primary = accent
//        disc, secondary = surface-elevated disc, hover/active surface-hover
//   :60  ring — border-strong (the port used border-muted)
//   :70  glyph tint — active accent; else primary WHITE, secondary
//        text-primary. The port painted the on-surface primary glyph BLACK,
//        which is the OTHER arm's colour.
//   :56  OVERLAY arm (`overlay: true`) — for buttons sitting on the dark
//        artwork atmosphere (the album / artist header once the header
//        gradient is on): primary = solid white disc with a BLACK glyph,
//        secondary = #ffffff24 fill (#ffffff3d hover/active), 1.5px
//        #ffffffcc ring, white glyph.
// NOTE: the card hover-overlay circles are still the separate
// CardOverlayButton.qml — same palette, different sizing contract.
//
// --- LIGHT-THEME LEGIBILITY (Slint 2.0.2, "CircleAction buttons legible on
// --- light themes" — docs/release-2.0.2/CHANGELOG.md:213) ----------------
// The `on-surface` arm IS that fix: CircleAction.slint:30-37 exists exactly
// because "the default light-on-dark palette (white fills/rings/icons)
// vanishes on light themes' plain surface". Its secondary glyph is
// `Theme.text-primary` (CircleAction.slint:73) — the theme's max-contrast
// colour, DARK on a light theme.
//
// The port could not express that, and this is where it was still broken:
// icon tints here are PRE-BAKED SVG variants (QbzIcon.qml), and the
// "primary" bake is a hardcoded `fill="#ffffff"` — it is NOT text-primary,
// it is white. So the on-surface secondary (a `surface-elevated` disc, i.e.
// near-white on a light theme) was painting a WHITE glyph on it: the exact
// vanishing act the .slint arm was written to stop. `onSurfaceTint` below
// resolves text-primary to the only baked dark variant ("black") whenever
// the active theme is light — theme.isDark is Rust-published from the
// surface-main luminance (theme_qt.rs:221), the same luminance test Slint's
// `Theme.is-dark` uses. The OVERLAY arm keeps its literals: it sits on the
// artwork band, which is dark under every theme.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root
    property string name: ""
    property bool active: false
    property bool primary: false
    property bool btnEnabled: true
    /// Light-on-dark palette for a button painted over artwork/atmosphere
    /// (CircleAction.slint's default arm; `on-surface: false` there).
    property bool overlay: false
    signal clicked(var mouse)

    QbzTheme { id: theme }

    /// Slint's `Theme.text-primary` glyph tint, expressed in baked variants:
    /// the "primary" bake is a literal #ffffff, so a light theme has to take
    /// the "black" bake or the glyph paints white-on-near-white.
    readonly property string onSurfaceTint: theme.isDark ? "primary" : "black"

    width: primary ? 44 : 32
    height: primary ? 44 : 32
    radius: width / 2
    color: overlay
        ? (primary
            ? (cbArea.containsMouse && btnEnabled ? "#d6ffffff" : "#ffffff")
            : ((cbArea.containsMouse || active) ? "#3dffffff" : "#24ffffff"))
        : (primary
            ? (cbArea.containsMouse && btnEnabled ? theme.accentHover : theme.accent)
            : ((cbArea.containsMouse || active) ? theme.surfaceHover : theme.surfaceElevated))
    border.width: primary ? 0 : 1.5
    border.color: overlay ? "#ccffffff" : theme.borderStrong
    opacity: btnEnabled ? 1.0 : 0.4
    QbzIcon {
        name: root.name
        width: root.primary ? 19 : 15
        height: root.primary ? 19 : 15
        anchors.centerIn: parent
        // CircleAction.slint:70-74, verbatim:
        //   active                  -> Theme.accent
        //   on-surface + primary    -> #ffffff      (glyph on the accent disc)
        //   on-surface + secondary  -> Theme.text-primary  <- the light-theme
        //                              case; "primary" would be white here
        //   overlay   + primary     -> #000000      (glyph on the white disc)
        //   overlay   + secondary   -> #ffffff
        tintName: root.active
            ? "accent"
            : (root.overlay
                ? (root.primary ? "black" : "primary")
                : (root.primary ? "primary" : root.onSurfaceTint))
    }
    MouseArea {
        id: cbArea
        anchors.fill: parent
        enabled: parent.btnEnabled
        hoverEnabled: true
        cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: function (mouse) { parent.clicked(mouse) }
    }
}
