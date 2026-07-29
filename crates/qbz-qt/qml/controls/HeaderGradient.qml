// HeaderGradient — the artwork-tinted band behind the album / artist page
// headers. ONE component, mounted by BOTH AlbumView and ArtistView (the
// Slint paints them from identical blocks: AlbumPageView.slint:235-257 and
// ArtistPageView.slint:225-247 are the same four lines twice).
//
// The Slint has TWO routes for this band:
//   route A  header-atmosphere.width > 0  -> ImmersiveAtmosphere (a blurred,
//            warped copy of the cover). GPU only.
//   route B  header-atmosphere.width <= 0 -> a flat linear gradient built
//            from `header-color` (crates/qbz/src/artwork.rs header_tint).
// Route A needs a shader; shader effects render NOTHING on this port's
// software path (the same finding that killed ColorOverlay in QbzIcon), so
// this component is route B — which is not an approximation, it is the exact
// fallback arm the Slint itself paints whenever the atmosphere image is
// missing. `tint` is the header_tint hex the Rust side now publishes in the
// page document (album_qt.rs / artist_qt.rs `headerColor`).
//
// Bands, read off the .slint:
//   band 1  linear-gradient(180deg, tint 0%, tint 16%, transparent 100%)
//   band 4  linear-gradient(180deg, transparent 0%, transparent 82%,
//                           surface-main 100%)
// Band 4 lands the fade EXACTLY on the header/content boundary whatever
// height the header resolved to, which is why the host binds `height` to the
// divider's y rather than to a constant.
//
// --- LIGHT-THEME CONTRAST FLOOR (bands 2 + 3) ---------------------------
// The header text is WHITE whenever this band is on — AlbumPageView.slint
// :169-172 pins `hdr-strong: #ffffff` / `hdr-body: #ffffff@88%` the moment
// `header-light` is true, on EVERY theme. That is only safe because the
// backdrop the .slint normally paints is route A, and route A is OPAQUE
// DARK: ImmersiveAtmosphere.slint:18 fills `#0a0a0b` under the artwork
// copies, then :76-79 lay a flat `#000000 @ dim` sheet (the album/artist
// headers pass `dim: 0.24`) and a `linear-gradient(180deg, #00000066 0%,
// transparent 35%, #00000080 100%)` vignette over the whole band. White text
// and the overlay-palette circles sit on that, not on the page colour.
//
// Route B alone does NOT reproduce that: its tint alpha decays from 1.0 at
// 16% to 0.0 at 100%, so below roughly half the band the backdrop IS the
// page. On a dark theme that is invisible (white on dark either way); on a
// LIGHT theme the description, the meta line and the whole CircleAction row
// — which live in the bottom third of the header — were white on white.
// Route A's two constant sheets are therefore replicated verbatim here as
// bands 2 and 3, and band 1 holds the tint solid until the 82% mark where
// band 4 takes over, matching route A's opaque coverage instead of route
// B's early decay. Same numbers, same order as ImmersiveAtmosphere.
//
// Mount it as the FIRST child of the page Flickable (so it scrolls with the
// content, as the Slint's does) at x:0 with the viewport width — the band is
// deliberately full-bleed: the page's own 32px padding must not clip it, or
// a dark gutter strip appears on the right (the .slint comment at
// AlbumPageView.slint:199-202 documents that exact regression).

import QtQuick
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    /// Artwork-derived tint, "#rrggbb" (header_tint). Empty = nothing paints.
    property string tint: ""
    /// The appearance pref AND the "no app-wide dynamic background" gate:
    /// `AppearanceState.album-header-gradient && !ShellState.app-background-active`
    /// (AlbumPageView.slint:168). The dynamic background and this atmosphere
    /// clash, so only one of them is ever on.
    property bool active: false
    /// ImmersiveAtmosphere's flat black sheet — the album and artist headers
    /// both mount it with `dim: 0.24` (AlbumPageView.slint:230,
    /// ArtistPageView.slint:222).
    property real dim: 0.24

    // `visible` gates the paint, but bindings still evaluate underneath it —
    // so the stops read this, never the raw string: coercing "" to a colour
    // logs a warning on every re-evaluation.
    readonly property color tintColor: root.tint === "" ? "transparent" : root.tint

    visible: root.active && root.tint !== ""
    // Never eat clicks meant for the header controls painted on top.
    enabled: false

    // Band 1 — the artwork tint. Solid down to the 82% mark (route A's image
    // field is opaque over exactly that span); the last 18% is band 4's job.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: root.tintColor }
            GradientStop { position: 0.16; color: root.tintColor }
            GradientStop { position: 0.82; color: root.tintColor }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    // Band 2 — ImmersiveAtmosphere.slint:77-79, `#000000.with-alpha(dim)`.
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, root.dim)
    }

    // Band 3 — ImmersiveAtmosphere.slint:80-82 vignette, verbatim stops.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#66000000" }
            GradientStop { position: 0.35; color: "transparent" }
            GradientStop { position: 1.0; color: "#80000000" }
        }
    }

    // Band 4 — the blend that must COMPLETE on the header/content divider.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 0.82; color: "transparent" }
            GradientStop { position: 1.0; color: theme.surfaceMain }
        }
    }
}
