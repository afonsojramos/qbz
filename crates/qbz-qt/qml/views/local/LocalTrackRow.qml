// Local track row — the SHARED rows/TrackRow.qml with the local arms set,
// plus the three things the shared row has no arm for:
//
//   * the always-visible SOURCE glyph the Slint renders from
//     `show-source: true` (LocalLibraryView.slint:1489) — drawn OUTSIDE the
//     shared row so TrackRow keeps one column layout for every surface;
//   * the multi-select checkbox (Slint TrackRow's `multi-select-mode`
//     swaps the number cell for it) — overlaid on the number cell here for
//     the same reason: no fork of the shared row, no shared-file edit;
//   * the LOCAL context menu.
//
// THE MENU (this is the `force-local-menu` arm of the unified Slint menu —
// TrackRow.slint:68 -> TrackMenuState -> AppShell.slint:1009-1021 ->
// TrackContextMenu.slint). For a local / Plex / offline row on a library
// surface Slint resolves to: qobuz-actions OFF, favorite OFF, Track info OFF
// (show-track-info rides the Qobuz block), mixtape ON, add-to-playlist ON,
// Go to album / Go to artist ON (`local-goto-actions`). So the full local set
// is: Play now, Play next, Play later, Add to queue, Add to mixtape, Add to
// playlist, Go to album, Go to artist.
//
// Add to mixtape and Add to playlist are OMITTED here, not disabled: this
// port has no Mixtape/Collection store and no playlist picker (the Rust side
// says so itself — `local_album_actions.rs` keeps them as LOGGED SEAMS), and
// a menu row that silently does nothing is worse than an absent one. Every
// row that IS shown is wired.
//
// The shared row's own ⋯ is turned OFF (`showMenu: false`) because its
// entries are the Qobuz set: its Go-to-album/artist call QbzAlbum.openAlbum /
// QbzArtist.openArtist with CATALOG ids, and a local row carries a local
// group key + a bare artist NAME. The button is re-drawn here, in the exact
// same gutter (the shared row is narrowed by precisely the 32px cell + 14px
// gap it no longer draws), and both it and a right-click open this menu.
//
// Artwork follows the Slint gate (AppearanceState.local-library-track-
// artwork, default OFF) — decoding a cover per row is exactly what froze
// this surface at 16k tracks.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
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

    // Local rows have a local album GROUP KEY and a bare artist NAME; both
    // routes are name/key routes, so each entry is gated on its own datum.
    readonly property bool canGoAlbum: (item.albumId || "") !== ""
    readonly property bool canGoArtist: (item.artist || "") !== ""

    TrackRow {
        id: sharedRow
        // 26px source-glyph gutter + the 46px (32 cell + 14 gap) the shared
        // row would have spent on its own ⋯ — so every other column lands
        // exactly where it did before the menu moved out.
        width: root.width - 26 - 46
        item: root.item
        number: root.number
        showArtwork: root.showArtwork
        showAlbum: root.showAlbum
        showFavorite: false
        showDownload: false
        showMenu: false
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

    // ⋯ menu button — the shared row's own gutter, re-drawn with the local
    // action set behind it.
    Rectangle {
        id: moreBtn
        x: root.width - 70
        width: 32
        height: 32
        radius: theme.radiusSm
        anchors.verticalCenter: parent.verticalCenter
        color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
        QbzIcon {
            name: "ellipsis"
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: moreArea.containsMouse ? "primary" : "muted"
        }
        MouseArea {
            id: moreArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: function (mouse) { rowMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
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

    // Right-click anywhere on the row opens the SAME menu at the pointer
    // (TrackRow.slint's `open-track-menu(..., at-pointer: true)`). Declared
    // last so it sits on top, and RIGHT-only so every left click still falls
    // through to the row body / checkbox / ⋯ underneath.
    MouseArea {
        id: rcArea
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        onClicked: function (mouse) { rowMenu.openAtCursor(rcArea, mouse.x, mouse.y) }
    }

    CardMenu {
        id: rowMenu
        menuWidth: 220
        entries: {
            var m = [
                { "label": QbzSession.tr("Play now", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
            ]
            if (root.canGoAlbum) m.push({ "label": QbzSession.tr("Go to album", QbzSession.trRev), "icon": "disc-3", "action": "go-album" })
            if (root.canGoArtist) m.push({ "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" })
            return m
        }
        onPicked: function (a) {
            if (a === "play") root.playRequested()
            else if (a === "go-album") {
                QbzLocal.openAlbum(root.item.albumId)
                QbzShell.navigateTo("localalbum")
            } else if (a === "go-artist") {
                // Local/Plex artists have no catalog id — a NAME route into
                // the Local Library Artists tab (local_album_actions.rs).
                QbzLocal.openArtistByName(root.item.artist)
                QbzShell.navigateTo("local")
            } else {
                root.enqueueRequested(a)
            }
        }
    }
}
