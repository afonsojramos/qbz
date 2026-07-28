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
// Favorite toggling and Go-to-artist/album are identical on every site —
// handled internally. The row BODY is the drag source (6px threshold,
// ghost + sidebar drops in main.rs).
//
// Deliberately NOT consolidated here (Slint-distinct): QueuePanel.QueueRow
// (QueueItem data + queue-op menu + press-and-hold reorder) and
// LibraryView.FeedListRow (mixed-kind feed row — inline in Slint too).

import QtQuick
import com.blitzfc.qbz
import "../controls"
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
                    onClicked: QbzBridge.openArtist(root.item.artistId)
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
                onClicked: if (root.item.albumId) QbzBridge.openAlbum(root.item.albumId)
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
        menuWidth: 220
        entries: {
            var m = [
                { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
            ]
            if (root.menuShowLater) m.push({ "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" })
            m.push({ "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" })
            if (root.menuShowGoTo && root.item.artistId) m.push({ "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" })
            if (root.menuShowGoTo && root.item.albumId) m.push({ "label": QbzSession.tr("Go to album", QbzSession.trRev), "icon": "disc", "action": "go-album" })
            if (root.menuShowFavorite) m.push({
                "label": root.item.isFavorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                "icon": root.item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
            if (root.menuShowRemove) m.push({ "label": QbzSession.tr("Remove from playlist", QbzSession.trRev), "icon": "trash-2", "action": "remove" })
            return m
        }
        onPicked: function (a) {
            if (a === "play") root.playRequested()
            else if (a === "next") root.enqueueRequested("next")
            else if (a === "later") root.enqueueRequested("later")
            else if (a === "queue") root.enqueueRequested("queue")
            else if (a === "go-artist") QbzBridge.openArtist(root.item.artistId)
            else if (a === "go-album") QbzBridge.openAlbum(root.item.albumId)
            else if (a === "favorite") {
                root.item.isFavorite = !root.item.isFavorite
                QbzLibrary.libraryToggleFavorite("track", root.item.id)
            } else if (a === "remove") root.removeRequested()
        }
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
        cursorShape: root.clickPlays ? Qt.PointingHandCursor : Qt.ArrowCursor
        onPressed: function (mouse) { root.downPos = Qt.point(mouse.x, mouse.y) }
        onPositionChanged: function (mouse) {
            if (!pressed || !root.draggable) return
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
        onDoubleClicked: root.playRequested()
    }
}
