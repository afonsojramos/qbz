// Albums tab body (LocalLibraryView.slint:1133).
//
// States, in the Slint's own order: initial spinner, load error + Retry,
// "nothing indexed yet", "nothing matches" (search OR the quality/format/
// source filter emptied the visible set), then the chunked collection with
// the bulk bar and the A-Z jump strip.
//
// Deviation: the Slint scrolls its MultiSelectBar away with the grid (it
// lives inside the scrolling page); here the bar is pinned above the
// collection, because the collection IS the scrolling view (one windowing
// ListView) rather than a page inside a Flickable.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    // Loading = the SHAPE of the collection that is coming (the Slint mounts
    // a bare 36px LoadingSpinner here), matching the tab's current view mode:
    // the 220x266 card grid or the 56px list rows. ONE instance for the whole
    // viewport — one animator, not one per cell (QbzSkeleton COST note).
    QbzSkeleton {
        visible: QbzLocal.localAlbumsLoading && root.view.albumsView === "grid"
        variant: "cardGrid"
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 16
        cellW: 220
        cellH: 266
        phase: root.view.skelPhase
    }
    QbzSkeleton {
        visible: QbzLocal.localAlbumsLoading && root.view.albumsView !== "grid"
        variant: "rowList"
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 16
        rowH: 56
        rowGap: 0
        rowArtSize: 40
        phase: root.view.skelPhase
    }

    // Load error + retry.
    Column {
        visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError !== ""
        anchors.centerIn: parent
        spacing: 10
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzSession.tr("Couldn't load your local albums.", QbzSession.trRev)
            color: theme.textPrimary
            font.pixelSize: theme.fontBody
            font.weight: theme.weightSemibold
        }
        SettingsButton {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzSession.tr("Retry", QbzSession.trRev)
            onClicked: QbzLocal.loadTab("albums")
        }
    }

    LocalNote {
        visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError === ""
            && root.view.albums.length === 0 && root.view.albumsSearch === ""
        text: QbzSession.tr("No albums in your local library yet.", QbzSession.trRev)
    }
    LocalNote {
        visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError === ""
            && root.view.albums.length > 0 && root.view.albumsVisible.length === 0
        text: QbzSession.tr("No albums match your search.", QbzSession.trRev)
    }

    Column {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 16
        spacing: 8
        visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError === ""
            && root.view.albumsVisible.length > 0

        QbzMultiSelectBar {
            visible: root.view.albumsMultiSelect
            width: parent.width
            selectedCount: root.view.albumsSelectedCount
            actions: [
                { "id": "select-all", "label": QbzSession.tr("Select all", QbzSession.trRev), "icon": "square-check-big", "danger": false, "needsSelection": false },
                { "id": "queue", "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "danger": false, "needsSelection": true },
                { "id": "play-later", "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "danger": false, "needsSelection": true },
                { "id": "play-next", "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "danger": false, "needsSelection": true },
                { "id": "add-to-playlist", "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "danger": false, "needsSelection": true },
                { "id": "add-to-mixtape", "label": QbzSession.tr("Add to Mixtape/Collection", QbzSession.trRev), "icon": "cassette-tape", "danger": false, "needsSelection": true },
                { "id": "clear", "label": QbzSession.tr("Clear", QbzSession.trRev), "icon": "x", "danger": false, "needsSelection": true },
            ]
            onAction: function (id) { root.view.albumsBulkAction(id) }
        }

        Item {
            width: parent.width
            height: parent.height - (root.view.albumsMultiSelect ? 52 : 0)

            LocalAlbumCollection {
                id: collection
                anchors.fill: parent
                anchors.rightMargin: root.view.albumsGroup === "alpha" ? 20 : 0
                view: root.view
                surface: "albums"
                rows: root.view.albumsVisible
                groups: root.view.albumsGrouped
                grouped: root.view.albumsGroup !== "off"
                viewMode: root.view.albumsView
                showSource: true
                selectMode: root.view.albumsMultiSelect
                selected: root.view.albumsSelected
                onOpenRequested: function (id) { root.view.openAlbum(id) }
                onPlayRequested: function (id) { QbzLocal.playAlbum(id, false) }
                onEnqueueRequested: function (id, m) { QbzLocal.enqueue("album", id, m) }
                onToggleSelect: function (id) { root.view.toggleAlbumSelected(id) }
            }
            QbzAlphaStrip {
                visible: root.view.albumsGroup === "alpha"
                    && collection.alphaJumps.length > 0
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                jumps: collection.alphaJumps
                onJump: function (ordinal, index) { collection.jumpToEntry(index) }
            }
        }
    }
}
