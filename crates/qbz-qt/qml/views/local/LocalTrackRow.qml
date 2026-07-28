// Local track row — the SHARED rows/TrackRow.qml with the local arms set,
// plus the two things the shared row has no arm for:
//
//   * the always-visible SOURCE glyph the Slint renders from
//     `show-source: true` (LocalLibraryView.slint:1489) — drawn OUTSIDE the
//     shared row so TrackRow keeps one column layout for every surface;
//   * the multi-select checkbox (Slint TrackRow's `multi-select-mode`
//     swaps the number cell for it) — overlaid on the number cell here for
//     the same reason: no fork of the shared row, no shared-file edit.
//
// Artwork follows the Slint gate (AppearanceState.local-library-track-
// artwork, default OFF) — decoding a cover per row is exactly what froze
// this surface at 16k tracks.

import QtQuick
import com.blitzfc.qbz
import "../../rows"
import "../../theme"

Item {
    id: root

    property var item: ({})
    property int number: 0
    property bool showAlbum: true
    property bool showArtwork: false
    property string artSource: ""
    property bool selectMode: false
    property bool checked: false
    signal playRequested()
    signal enqueueRequested(string mode)
    signal toggleSelect()

    QbzTheme { id: theme }

    height: 50

    TrackRow {
        id: sharedRow
        width: root.width - 26
        item: root.item
        number: root.number
        showArtwork: root.showArtwork
        showAlbum: root.showAlbum
        showFavorite: false
        showDownload: false
        menuShowGoTo: false
        menuShowFavorite: false
        // Local rows are NOT drag sources: item.id is a local DB row id and
        // the sidebar drop handler forwards it as a Qobuz catalog id.
        draggable: false
        qualityStyle: "icon"
        onPlayRequested: root.playRequested()
        onEnqueueRequested: function (m) { root.enqueueRequested(m) }
    }

    // Multi-select checkbox over the number cell (x 12..44 in TrackRow).
    Rectangle {
        visible: root.selectMode
        x: 12
        width: 32
        height: 40
        anchors.verticalCenter: parent.verticalCenter
        color: theme.surfaceMain
        radius: 4
        SelectCheck {
            anchors.centerIn: parent
            on: root.checked
            onToggled: root.toggleSelect()
        }
    }

    // Source glyph (local / offline cache / Plex).
    QbzIcon {
        visible: root.item.source === "local" || root.item.source === "plex"
            || root.item.source === "offline"
        name: root.item.source === "offline" ? "cloud-download" : "hard-drive"
        width: 14
        height: 14
        x: root.width - 20
        anchors.verticalCenter: parent.verticalCenter
        tintName: root.item.source === "plex" ? "accent" : "muted"
    }
}
