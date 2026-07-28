// Per-version source glyph (album/LocalAlbumView.slint:23 SourceIcon):
// hard-drive for a local file, the Qobuz mark for an offline/purchased copy
// (gold when purchased), the Plex mark for Plex.
//
// ASSET GAP: the Qt icon set has no qobuz-logo-filled / plex-logo variant
// yet (see GLUE), so the offline copy reuses cloud-download and Plex reuses
// hard-drive in the accent tint — the same substitution the Local Library
// cards already ship.

import QtQuick
import "../../theme"

QbzIcon {
    id: root

    /// "local" | "qobuz_download" | "qobuz_purchase" | "offline" | "plex"
    property string kind: "local"

    width: 14
    height: 14
    name: (kind === "qobuz_download" || kind === "qobuz_purchase" || kind === "offline")
        ? "cloud-download" : "hard-drive"
    tintName: kind === "qobuz_purchase" ? "warning"
        : kind === "plex" ? "accent" : "secondary"
}
