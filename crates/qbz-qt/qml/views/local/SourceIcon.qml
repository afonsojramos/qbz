// Per-version / per-row source glyph: hard-drive for a local file, the Qobuz
// mark for an offline/purchased copy (gold when purchased), the Plex mark for
// Plex.
//
// ── IT STANDS IN FOR TWO SLINT COMPONENTS, WHICH DISAGREE ──────────────────
// `album/LocalAlbumView.slint:22-37` (`SourceIcon`) — the VERSION PICKER's
// glyph: 14px, and the monochrome local glyph tinted `Theme.text-SECONDARY`.
// `primitives/SourceGlyph.slint:15-32` — the shared row glyph used by the MyQBZ
// collection-detail rows (`MixtapeDetailView.slint:370`), the Discography
// Builder rows (`DiscographyBuilderView.slint:54`), FavoritesView and TrackRow:
// 15px, and the local glyph tinted `Theme.text-MUTED`, never accent.
// (They also disagree on plain `"qobuz"`: LocalAlbumView lets it fall through to
// hard-drive, SourceGlyph gives it the Qobuz mark. This file follows SourceGlyph
// there — see `kind` below.)
//
// So the two numbers that differ are PROPERTIES, and the DEFAULTS are the
// version-picker set, which is what `views/local/VersionPicker.qml` (the only
// two call sites that set neither) needs. A row that stands for SourceGlyph
// passes `glyphSize: 15` and `localTint: "muted"`.
//
// SourceGlyph's other size rule — 16px for the three qobuz kinds
// (`SourceGlyph.slint:29`) — is deliberately NOT reproduced: it exists to
// compensate the Qobuz WORDMARK's proportions, and this port does not draw that
// glyph (see the asset gap). Widening a `cloud-download` substitute to 16 would
// be a number with no reference behind it.
//
// ASSET GAP: the Qt icon set has NO `qobuz-logo-filled.svg` and NO
// `plex-logo.svg` — neither an untinted master nor any tint directory
// (`qml/assets/icons/{accent,amber,black,favorite,muted,primary,secondary,warning}/`
// and `qml/assets/brand/` all verified). Both are present in the Slint tree
// (`crates/qbz-ui/ui/assets/icons/`). Until they are ported the offline/purchased
// copy reuses `cloud-download` and Plex reuses `hard-drive` in the accent tint —
// the same substitution the Local Library cards already ship — and the "logos
// render UNTINTED" rule those two arms exist for cannot be honoured at all,
// because a substitute monochrome glyph has to be tinted to read.

import QtQuick
import "../../theme"

QbzIcon {
    id: root

    /// "local" | "qobuz" | "qobuz_download" | "qobuz_purchase" | "offline" | "plex"
    ///
    /// Plain "qobuz" is what the MyQBZ collection items carry (a streamed Qobuz
    /// album/track, not a download or a purchase). Without its own arm it fell
    /// through to "hard-drive", i.e. every Qobuz row in a collection claimed to
    /// be a local file. `SourceGlyph.slint:23` groups it with the downloads for
    /// the same reason.
    property string kind: "local"

    /// 14 = album/LocalAlbumView.slint:24 (the version picker).
    /// 15 = primitives/SourceGlyph.slint:29-30 (every row glyph).
    property int glyphSize: 14
    /// Tint of the MONOCHROME local hard-drive glyph. "secondary" =
    /// LocalAlbumView.slint:36; "muted" = SourceGlyph.slint:28, whose header
    /// calls the muted-never-accent rule load-bearing.
    property string localTint: "secondary"

    width: root.glyphSize
    height: root.glyphSize
    name: (kind === "qobuz" || kind === "qobuz_download"
           || kind === "qobuz_purchase" || kind === "offline")
        ? "cloud-download" : "hard-drive"
    // `warning` for the purchase gold (#eab308 in both Slint components) and
    // `accent` to keep the Plex substitute distinguishable from a local file;
    // everything else is the monochrome local glyph.
    tintName: kind === "qobuz_purchase" ? "warning"
        : kind === "plex" ? "accent"
        : (kind === "qobuz" || kind === "qobuz_download" || kind === "offline")
            ? "secondary"
            : root.localTint
}
