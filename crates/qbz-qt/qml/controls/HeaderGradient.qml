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
// Two stacked bands, read off the .slint:
//   band 1  linear-gradient(180deg, tint 0%, tint 16%, transparent 100%)
//   band 2  linear-gradient(180deg, transparent 0%, transparent 82%,
//                           surface-main 100%)
// Band 2 lands the fade EXACTLY on the header/content boundary whatever
// height the header resolved to, which is why the host binds `height` to the
// divider's y rather than to a constant.
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

    // `visible` gates the paint, but bindings still evaluate underneath it —
    // so the stops read this, never the raw string: coercing "" to a colour
    // logs a warning on every re-evaluation.
    readonly property color tintColor: root.tint === "" ? "transparent" : root.tint

    visible: root.active && root.tint !== ""
    // Never eat clicks meant for the header controls painted on top.
    enabled: false

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: root.tintColor }
            GradientStop { position: 0.16; color: root.tintColor }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 0.82; color: "transparent" }
            GradientStop { position: 1.0; color: theme.surfaceMain }
        }
    }
}
