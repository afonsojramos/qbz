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
// The port could not express that at first: icon tints were PRE-BAKED SVG
// variants only, and the "primary" bake is a hardcoded `fill="#ffffff"` — it
// is NOT text-primary, it is white. So the on-surface secondary (a
// `surface-elevated` disc, i.e. near-white on a light theme) painted a WHITE
// glyph on it: the exact vanishing act the .slint arm was written to stop.
// The stopgap was `onSurfaceTint = theme.isDark ? "primary" : "black"`, a
// 2-value approximation of text-primary.
//
// It is no longer needed. QbzIcon resolves theme-following tints through
// src/icon_tint_qt.rs, which bakes the live theme's ACTUAL text-primary hex,
// so this arm now says "textPrimary" and is exact under all 36 themes.
//
// The OVERLAY arm keeps its literals: it sits on hosts that are
// theme-independent — the artwork band, the solid white disc — and Slint
// hardcodes them for the same reason (CircleAction.slint:70-74, `#ffffff` /
// `#000000` literals next to `Theme.text-primary`). It spells them
// "white"/"black" now, not "primary", so no one re-reads the legacy alias as
// the theme token.
//
// The on-surface PRIMARY glyph USED to be in that group and is not any more.
// Its host is NOT theme-independent — it is `theme.accent` — and the fixed
// #ffffff CircleAction.slint:72 puts on it is unreadable on 16 of the 35
// shipped palettes (worst: high-contrast 1.70:1, then ikari 1.74, wcag-dark
// 1.82). It now takes `theme.accentGlyphTint`, the measured
// on-accent selector; see the note at the tint binding and the long rationale
// in theme/QbzTheme.qml.

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

    /// Slint's `Theme.text-primary` glyph tint (CircleAction.slint:73),
    /// runtime-tinted so it is the real token under every theme.
    readonly property string onSurfaceTint: "textPrimary"

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
        // Whole-pixel centring, both axes — CircleAction.slint:67-68 rounds
        // them explicitly for the same reason: 44-19 and 32-15 are both ODD,
        // so `anchors.centerIn` lands the glyph on 12.5px / 8.5px and the
        // software renderer resamples it soft, right next to the now
        // pixel-crisp 28px QbzNavButton chevrons.
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        // CircleAction.slint:70-74:
        //   active                  -> Theme.accent
        //   on-surface + primary    -> #ffffff  <- DELIBERATE DIVERGENCE, see
        //                              below; this port uses the measured
        //                              on-accent selector instead
        //   on-surface + secondary  -> Theme.text-primary  <- the light-theme
        //                              case; a fixed white would vanish here
        //   overlay   + primary     -> #000000      (glyph on the white disc)
        //   overlay   + secondary   -> #ffffff
        //
        // The ONE divergence is the on-surface PRIMARY glyph. Its host is the
        // 44px `theme.accent` disc (`color:` above, accentHover on hover —
        // which is why the selector folds both fills in) and CircleAction.slint:72
        // hardcodes #ffffff on it — which measures 1.70:1 on high-contrast,
        // 1.74 on ikari, 1.82 on wcag-dark, 1.85 on kurosaki and so on for
        // 16 of the 35 shipped palettes (the two accessibility themes lead
        // that list, which is the strongest part of the argument).
        // `theme.accentGlyphTint` is that decision made against the
        // live accent (QbzTheme.qml, "ON AN ACCENT FILL"); it still resolves
        // to a white glyph on Dark / Light / OLED, so the default themes'
        // play button is unchanged.
        //
        // The OVERLAY arm keeps its literals: those hosts (the solid white
        // disc, the artwork band) are theme-independent, so there is nothing
        // to measure against and nothing to diverge from.
        tintName: root.active
            ? "accent"
            : (root.overlay
                ? (root.primary ? "black" : "white")
                : (root.primary ? theme.accentGlyphTint : root.onSurfaceTint))
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
