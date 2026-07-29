// TrackRow — THE track list row (primitives/TrackRow.slint, POC arm
// subset), consolidated in phase 22 from FOUR copies (AlbumView
// .AlbumTrackRow, PlaylistView.PlTrackRow, LibraryView.TrackListRow,
// SearchView.SearchTrackRow). 50px, radius 8: number→play cell (pause
// swap + accent pill when it is the now-playing row — Slint-universal),
// optional 36px art cell, title+explicit / artist column, optional 220px
// album link, duration, quality (icon QualityMini | bare text), optional
// heart, optional download slot (offline column — glyph stub or reserved
// spacer until the offline cache lands, POC-NOTE), ⋯ CardMenu.
//
// item contract: { id, title, artist, artistId, album, albumId, number?,
// duration, qualityTier, explicit, isFavorite, artPath? } (plus
// playlistTrackId for the remove-from-playlist arm).
//
// Arms: showArtwork / showAlbum / showFavorite / showDownload /
// downloadGlyph / showMenu / zebra / artistLink / clickPlays /
// qualityStyle ("icon"|"text") / menuShowLater / menuShowGoTo /
// menuShowFavorite / menuShowRemove.
// Signals: playRequested() (per-site play: album-scoped, playlist-scoped
// or plain), enqueueRequested(mode) ("next"|"later"|"queue"),
// removeRequested(), bodyDragStarted(index) (fired BEFORE the shared
// dragStart — the #589 reorder pre-hook).
// Favorite toggling, Go-to-artist/album, Share and Track info are
// identical on every site — handled internally. The row BODY is the drag
// source (6px threshold, ghost + sidebar drops in main.rs) and its RIGHT
// press opens the very same menu the ⋯ button does.
//
// --- Menu inventory vs primitives/TrackContextMenu.slint ----------------
// The .slint menu is FLAT (no separators) in this order: Play now · Play
// next · Play later · Add to queue · Create QBZ radio · Create Qobuz radio ·
// Add to library · Add to mixtape · Add to playlist · Remove from
// playlist(danger) · Share Qobuz link · Share Song.link · Make available
// offline | Refresh + Remove offline copy(danger) · Go to album · Go to
// artist · Track info. Everything the Qt bridge has a seam for is here, in
// that order; the rest is gated OFF by the `has*Seam` constants below
// rather than rendered as a row that silently does nothing.
//
// Deliberately NOT consolidated here (Slint-distinct): QueuePanel.QueueRow
// (QueueItem data + queue-op menu + press-and-hold reorder) and
// LibraryView.FeedListRow (mixed-kind feed row — inline in Slint too).

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../shell"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    property int number: 0
    property bool showArtwork: false
    property bool showAlbum: false
    property bool showFavorite: true
    property bool showDownload: false
    property bool downloadGlyph: false
    property bool showMenu: true
    property bool zebra: false
    property bool artistLink: false
    property bool clickPlays: true
    property string qualityStyle: "icon"
    property bool menuShowLater: true
    property bool menuShowGoTo: true
    property bool menuShowFavorite: true
    property bool menuShowRemove: false
    // Drag source arm (additive; default = the existing behaviour). The
    // Local Library rows set this false: their `item.id` is a local DB row
    // id, and the sidebar drop handler forwards whatever it receives to
    // `playlist add-tracks` as a QOBUZ catalog id — a local row dropped on a
    // playlist would silently add an unrelated catalog track.
    property bool draggable: true
    // Per-row artwork placeholder (see the 36px cell below). The host view
    // owns the phase clock so one timer drives every row.
    property bool artPending: false
    property bool skelPhase: false
    property int artSettleMs: 0

    // Catalog-backed row? `draggable` is ALREADY the "item.id is a Qobuz
    // catalog track id" predicate (the Local Library / ephemeral rows set it
    // false because their id is a local DB row id — see the note above), so
    // the two entries that hit the Qobuz catalog by id ride it instead of
    // asking every host to pass a new arm.
    readonly property bool catalogRow: root.draggable
    property bool menuShowShare: root.catalogRow
    property bool menuShowTrackInfo: root.catalogRow

    // --- Seams the Qt bridge does NOT have yet (menu-parity round) -------
    // Each maps to a TrackContextMenu.slint entry. OFF = the row is not
    // built at all, never rendered-and-inert. Flip the constant AND fill the
    // matching `menuAction` branch when the invokable lands; the entry then
    // appears in its .slint-correct slot. Why each is missing:
    //   radio        no radio invokable on any Qt bridge object
    //   mixtape      QbzLocal.albumAddToMixtape is LOCAL-album only
    //   playlistAdd  no picker modal, no by-id add (only the sidebar drag)
    //   songlink     needs the ISRC -> Deezer -> Odesli round-trip (backend)
    //   offlineCache no per-track cache bridge (the download cell is a stub)
    readonly property bool hasRadioSeam: false
    readonly property bool hasMixtapeSeam: false
    readonly property bool hasPlaylistAddSeam: false
    readonly property bool hasSonglinkSeam: false
    readonly property bool hasOfflineCacheSeam: false

    signal playRequested()
    signal enqueueRequested(string mode)
    signal removeRequested()
    signal bodyDragStarted(int index)

    QbzTheme { id: theme }

    width: parent ? parent.width : 0
    height: 50
    radius: 8
    color: hovered ? theme.surfaceHover : (zebra && number % 2 === 0 ? "#07ffffff" : "transparent")

    readonly property bool hovered: trArea.containsMouse || favArea.containsMouse || moreArea.containsMouse
    readonly property bool isActive: QbzPlayer.npTrackId === (item.id || "")
    readonly property int cellsRight: 70 + 92 + (showFavorite ? 28 : 0) + (showDownload ? 28 : 0) + (showMenu ? 32 : 0)
    readonly property int cellsLeft: 32 + (showArtwork ? 36 : 0) + (showAlbum ? 220 : 0)
    readonly property int gaps: (3 + (showArtwork ? 1 : 0) + (showAlbum ? 1 : 0)
        + (showFavorite ? 1 : 0) + (showDownload ? 1 : 0) + (showMenu ? 1 : 0)) * 14

    // Static now-playing mark: 3px accent pill on the left edge.
    Rectangle {
        visible: root.isActive
        x: 2
        y: 7
        width: 3
        height: parent.height - 14
        radius: 1.5
        color: theme.accent
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: 14

        // Number cell — swaps to play on hover (pause when active+playing).
        Item {
            width: 32
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            Text {
                visible: !trArea.containsMouse
                anchors.centerIn: parent
                text: root.number
                color: theme.textMuted
                font.pixelSize: 13
            }
            Rectangle {
                visible: trArea.containsMouse
                anchors.centerIn: parent
                width: 28
                height: 28
                radius: 14
                color: root.isActive && QbzPlayer.npPlaying ? "transparent" : "#3dffffff"
                border.width: root.isActive && QbzPlayer.npPlaying ? 1.5 : 0
                border.color: theme.accent
                QbzIcon {
                    anchors.centerIn: parent
                    name: root.isActive && QbzPlayer.npPlaying ? "pause" : "play-fill"
                    width: 14
                    height: 14
                    tintName: root.isActive && QbzPlayer.npPlaying ? "accent" : "primary"
                }
            }
        }
        // 36px artwork cell (showArtwork arm).
        Rectangle {
            visible: root.showArtwork
            width: 36
            height: 36
            anchors.verticalCenter: parent.verticalCenter
            radius: 4
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                source: root.item.artPath || ""
                radius: 4
            }
            // Per-item cover placeholder — clears when THIS row's cover lands,
            // which is what makes a long list read as progressive instead of
            // filling in one lump. Host views drive the three properties;
            // default-off, so no existing call site changes.
            QbzSkeleton {
                variant: "art"
                anchors.fill: parent
                blockRadius: 4
                visible: root.artPending
                phase: root.skelPhase
                settleMs: root.artSettleMs
            }
        }
        // Title (+ explicit) / artist.
        Column {
            width: parent.width - root.cellsLeft - root.cellsRight - root.gaps
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Row {
                spacing: 6
                Text {
                    text: root.item.title || ""
                    color: theme.textPrimary
                    font.pixelSize: 14
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, parent.parent.width - (root.item.explicit ? 22 : 0))
                }
                Rectangle {
                    visible: root.item.explicit === true
                    width: 16
                    height: 16
                    radius: 3
                    anchors.verticalCenter: parent.verticalCenter
                    color: theme.surfaceElevated
                    Text {
                        anchors.centerIn: parent
                        text: "E"
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                    }
                }
            }
            Text {
                width: parent.width
                visible: (root.item.artist || "") !== ""
                text: root.item.artist || ""
                color: root.artistLink && root.item.artistId && artistLinkArea.containsMouse
                    ? theme.textPrimary : theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
                MouseArea {
                    id: artistLinkArea
                    anchors.fill: parent
                    enabled: root.artistLink && !!root.item.artistId
                    hoverEnabled: true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: QbzArtist.openArtist(root.item.artistId)
                }
            }
        }
        // Album (link) column (showAlbum arm).
        Text {
            visible: root.showAlbum
            width: 220
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.album || ""
            color: albumArea.containsMouse ? theme.accent : theme.textMuted
            font.pixelSize: 12
            elide: Text.ElideRight
            MouseArea {
                id: albumArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: root.item.albumId ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: if (root.item.albumId) QbzAlbum.openAlbum(root.item.albumId)
            }
        }
        // Duration.
        Text {
            width: 70
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.duration || ""
            color: theme.textMuted
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
        }
        // Quality (92px): icon (QualityMini) or bare text.
        Rectangle {
            width: 92
            height: parent.height
            color: "transparent"
            QualityMini {
                visible: root.qualityStyle === "icon"
                tier: root.item.qualityTier || ""
                anchors.centerIn: parent
            }
            Text {
                visible: root.qualityStyle === "text"
                anchors.centerIn: parent
                text: root.item.qualityTier === "hires" ? "HI-RES" : (root.item.qualityTier === "cd" ? "CD" : "")
                color: theme.textMuted
                font.pixelSize: 10
                font.weight: theme.weightBold
                horizontalAlignment: Text.AlignHCenter
            }
        }
        // Favorite (showFavorite arm).
        Rectangle {
            visible: root.showFavorite
            width: 28
            height: 28
            radius: theme.radiusSm
            anchors.verticalCenter: parent.verticalCenter
            color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                anchors.centerIn: parent
                name: root.item.isFavorite ? "heart-filled" : "heart"
                width: 16
                height: 16
                tintName: root.item.isFavorite ? "favorite" : (favArea.containsMouse ? "primary" : "muted")
            }
            MouseArea {
                id: favArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    root.item.isFavorite = !root.item.isFavorite
                    QbzLibrary.libraryToggleFavorite("track", root.item.id)
                }
            }
        }
        // Download slot (showDownload arm): inert glyph stub or reserved
        // spacer (the offline-cache column — not ported, POC-NOTE; the
        // Slint reserves the slot so the grid stays aligned).
        Item {
            visible: root.showDownload
            width: 28
            height: 28
            anchors.verticalCenter: parent.verticalCenter
            QbzIcon {
                visible: root.downloadGlyph
                anchors.centerIn: parent
                name: "cloud-download"
                width: 16
                height: 16
                tintName: "muted"
            }
        }
        // ⋯ menu (showMenu arm).
        Rectangle {
            visible: root.showMenu
            width: 32
            height: 32
            radius: theme.radiusSm
            anchors.verticalCenter: parent.verticalCenter
            color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                anchors.centerIn: parent
                name: "ellipsis"
                width: 16
                height: 16
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
    }
    CardMenu {
        id: rowMenu
        menuWidth: 224
        entries: root.menuModel()
        onPicked: function (a) { root.menuAction(a) }
    }

    // TrackContextMenu.slint, in its order. Every row here reaches a live
    // seam; the seamless ones are gated off above.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": t("Play now", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "next" },
        ]
        // #442 "Play later" — the end of the MANUAL block (after everything
        // already queued by hand, before the source resumes).
        if (root.menuShowLater)
            m.push({ "label": t("Play later", r), "icon": "list-plus", "action": "later" })
        m.push({ "label": t("Add to queue", r), "icon": "list-end", "action": "queue" })
        if (root.hasRadioSeam) {
            m.push({ "label": t("Create QBZ radio", r), "icon": "radio", "action": "radio-qbz" })
            m.push({ "label": t("Create Qobuz radio", r), "icon": "radio", "action": "radio-qobuz" })
        }
        if (root.menuShowFavorite)
            m.push({ "label": root.item.isFavorite ? t("Remove from Library", r) : t("Add to Library", r),
                     "icon": root.item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
        if (root.hasMixtapeSeam)
            m.push({ "label": t("Add to mixtape", r), "icon": "cassette-tape", "action": "mixtape" })
        if (root.hasPlaylistAddSeam)
            m.push({ "label": t("Add to playlist", r), "icon": "list-music", "action": "add-to-playlist" })
        if (root.menuShowRemove)
            m.push({ "label": t("Remove from playlist", r), "icon": "trash-2",
                     "action": "remove", "danger": true })
        if (root.menuShowShare)
            m.push({ "label": t("Share Qobuz link", r), "icon": "link", "action": "share-qobuz" })
        if (root.hasSonglinkSeam)
            m.push({ "label": t("Share Song.link", r), "icon": "link", "action": "share-songlink" })
        if (root.hasOfflineCacheSeam)
            m.push({ "label": t("Make available offline", r), "icon": "cloud-download", "action": "cache" })
        if (root.menuShowGoTo && root.item.albumId)
            m.push({ "label": t("Go to album", r), "icon": "disc-3", "action": "go-album" })
        if (root.menuShowGoTo && root.item.artistId)
            m.push({ "label": t("Go to artist", r), "icon": "user", "action": "go-artist" })
        if (root.menuShowTrackInfo)
            m.push({ "label": t("Track info", r), "icon": "info", "action": "track-info" })
        return m
    }

    function menuAction(a) {
        if (a === "play") root.playRequested()
        else if (a === "next") root.enqueueRequested("next")
        else if (a === "later") root.enqueueRequested("later")
        else if (a === "queue") root.enqueueRequested("queue")
        else if (a === "go-artist") QbzArtist.openArtist(root.item.artistId)
        else if (a === "go-album") QbzAlbum.openAlbum(root.item.albumId)
        else if (a === "favorite") {
            root.item.isFavorite = !root.item.isFavorite
            QbzLibrary.libraryToggleFavorite("track", root.item.id)
        } else if (a === "remove") root.removeRequested()
        else if (a === "share-qobuz") root.copyToClipboard(
            "https://open.qobuz.com/track/" + (root.item.id || ""))
        else if (a === "track-info") root.openTrackInfo()
    }

    // --- Share (share.rs::qobuz_track_url + copy_to_clipboard) -----------
    // The .slint arm copies the link and raises a toast; the Qt port has no
    // toast seam yet (GLUE NEEDED), so the copy is silent — but it IS a real
    // clipboard write. QtQuick exposes no Clipboard type; TextEdit.copy() is
    // the supported route, kept in an INACTIVE Loader so a 16K-row list does
    // not carry one TextEdit per row.
    Loader {
        id: clipLoader
        active: false
        sourceComponent: TextEdit { visible: false }
    }
    function copyToClipboard(text) {
        if (!text || text === "")
            return
        clipLoader.active = true
        clipLoader.item.text = text
        clipLoader.item.selectAll()
        clipLoader.item.copy()
        clipLoader.active = false
    }

    // --- Track info (QbzAlbum.openTrackInfo + shell/TrackInfoModal) ------
    // Same lazy-Loader reason: the modal is a full Popup tree and a list row
    // must not instantiate one per delegate. Activated on demand, torn down
    // on close (Qt.callLater so the Popup is not destroyed mid-signal).
    Loader {
        id: trackInfoLoader
        active: false
        sourceComponent: TrackInfoModal { }
    }
    Connections {
        target: trackInfoLoader.item
        ignoreUnknownSignals: true
        function onClosed() { Qt.callLater(function () { trackInfoLoader.active = false }) }
    }
    function openTrackInfo() {
        if (!root.item.id)
            return
        trackInfoLoader.active = true
        trackInfoLoader.item.openFor(root.item.id)
    }

    // Shared drag (the row BODY is the source — TrackRow.slint): press-drag
    // >6px starts it (bodyDragStarted fires FIRST, the #589 pre-hook);
    // release plays (clickPlays) or ignores (album view: double-click).
    property bool dragging: false
    property point downPos: Qt.point(0, 0)
    MouseArea {
        id: trArea
        anchors.fill: parent
        hoverEnabled: true
        propagateComposedEvents: true
        // Right press opens the SAME menu as ⋯ (rowMenu), at the pointer.
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: root.clickPlays ? Qt.PointingHandCursor : Qt.ArrowCursor
        onPressed: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                if (root.showMenu)
                    rowMenu.openAtCursor(trArea, mouse.x, mouse.y)
                mouse.accepted = true
                return
            }
            root.downPos = Qt.point(mouse.x, mouse.y)
        }
        onPositionChanged: function (mouse) {
            // Only a LEFT press drags — a right press is the context gesture.
            if (!pressed || !(pressedButtons & Qt.LeftButton) || !root.draggable) return
            const g = mapToItem(null, mouse.x, mouse.y)
            if (!root.dragging
                && (Math.abs(mouse.x - root.downPos.x) > 6
                    || Math.abs(mouse.y - root.downPos.y) > 6)) {
                root.dragging = true
                root.bodyDragStarted(root.number)
                QbzShell.dragStart(root.item.id, root.item.title || "",
                    (root.item.artist || "") + " · " + (root.item.album || ""), g.x, g.y)
            }
            if (root.dragging) QbzShell.dragMove(g.x, g.y)
        }
        onReleased: function (mouse) {
            if (mouse.button === Qt.RightButton)
                return
            if (root.dragging) {
                QbzShell.dragEnd()
                root.dragging = false
                mouse.accepted = true
            } else if (root.clickPlays) {
                root.playRequested()
            } else {
                mouse.accepted = false
            }
        }
        onDoubleClicked: function (mouse) {
            if (mouse.button === Qt.LeftButton)
                root.playRequested()
        }
    }
}
