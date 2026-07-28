// TrackCard — THE track card (discover/TrackCard.slint), promoted from
// LibraryView.LibTrackCard in phase 21: Library Tracks grid + All feed.
// 200x246: 200px cover + hover scrim 0.6, overlay row (heart / play /
// more) at y=120, optional source badge, then the meta row: title +
// "Track • Artist" (artist links out) + the icon-only quality badge.
// Body click PLAYS (the TrackCard convention, unlike album/playlist).
//
// item contract: { id, title, artist, artistId, albumId, qualityTier,
// isFavorite, source } plus the host-resolved artSource string prop.
//
// NOTE the Search most-popular track hero is NOT this card (Slint:
// primitives/SearchTrackHero.slint — a distinct 160x220 filled-chrome
// hero); the POC keeps its own SearchTrackHero variant (200x246, centered
// play, quality as text) in SearchView.qml with a justifying comment.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    // Host-resolved artwork path (the AlbumCard artSource pattern).
    property string artSource: ""
    // Remote cover URL for the pin payload ("" when the host has none).
    property string artworkUrl: ""
    // ADR-008 source glyph (local/Plex) — the Library All-feed arm.
    property bool showSourceBadge: false

    color: "transparent"

    QbzTheme { id: theme }

    readonly property bool overlayOn: tcArtArea.containsMouse || favBtn.hovered
        || playBtn.hovered || moreBtn.hovered

    implicitWidth: 200
    implicitHeight: 246

    Column {
        spacing: 0
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: theme.radiusSm
            }
            Rectangle {
                anchors.fill: parent
                radius: theme.radiusSm
                color: "#000000"
                opacity: root.overlayOn ? 0.6 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }
            // Body click PLAYS the track (TrackCard hover).
            MouseArea {
                id: tcArtArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzPlayer.playTrack(root.item.id)
            }
            // Hover overlay — favorite / play / more (y=120, h=44,
            // centered, spacing 12).
            CardOverlayRow {
                y: 120
                width: parent.width
                shown: root.overlayOn
                CardOverlayButton {
                    id: favBtn
                    name: root.item.isFavorite ? "heart-filled" : "heart"
                    active: root.item.isFavorite
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: {
                        root.item.isFavorite = !root.item.isFavorite
                        QbzLibrary.libraryToggleFavorite("track", root.item.id)
                    }
                }
                CardOverlayButton {
                    id: playBtn
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: QbzPlayer.playTrack(root.item.id)
                }
                CardOverlayButton {
                    id: moreBtn
                    name: "ellipsis"
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: function (mouse) { trackMenu.openAtCursor(moreBtn, mouse.x, mouse.y) }
                }
            }
            // Source badge (All feed, show-local): bottom-right.
            Rectangle {
                visible: root.showSourceBadge && (root.item.source === "local" || root.item.source === "plex")
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                color: "#b3000000"
                QbzIcon {
                    name: "hard-drive"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: root.item.source === "plex" ? "accent" : "primary"
                }
            }
            CardMenu {
                id: trackMenu
                menuWidth: 196
                entries: root.menuModel()
                onPicked: function (a) { root.trackAction(a) }
            }
        }
        Item { width: 1; height: 6 }
        // Title / "Track • Artist" + quality badge.
        Row {
            width: 200
            height: 40
            spacing: theme.spacingSm
            Column {
                width: parent.width - (tcQ.visible ? tcQ.width + theme.spacingSm : 0)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 20
                    text: root.item.title || ""
                    color: tcTitleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: tcTitleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzPlayer.playTrack(root.item.id)
                    }
                }
                Text {
                    width: parent.width
                    height: 18
                    text: QbzSession.tr("Track", QbzSession.trRev) + " • " + (root.item.artist || "")
                    color: root.item.artistId && tcArtistArea.containsMouse
                        ? theme.textPrimary : theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: tcArtistArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.item.artistId ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (root.item.artistId) QbzArtist.openArtist(root.item.artistId)
                    }
                }
            }
            QualityMini { id: tcQ; tier: root.item.qualityTier || ""; anchors.verticalCenter: parent.verticalCenter }
        }
    }

    // Track context-menu model (TrackCard.slint track-menu) + dispatch.
    function menuModel() {
        var m = [
            { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
            { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
            { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
            { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
        ]
        if (root.item.artistId) m.push({ "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" })
        if (root.item.albumId) m.push({ "label": QbzSession.tr("Go to album", QbzSession.trRev), "icon": "disc", "action": "go-album" })
        m.push({ "label": root.item.isFavorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                 "icon": root.item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
        return m
    }
    function trackAction(a) {
        if (a === "play") QbzPlayer.playTrack(root.item.id)
        else if (a === "next") QbzPlayer.enqueueTrack(root.item.id, "next")
        else if (a === "later") QbzPlayer.enqueueTrack(root.item.id, "later")
        else if (a === "queue") QbzPlayer.enqueueTrack(root.item.id, "queue")
        else if (a === "go-artist") QbzArtist.openArtist(root.item.artistId)
        else if (a === "go-album") QbzAlbum.openAlbum(root.item.albumId)
        else if (a === "favorite") {
            root.item.isFavorite = !root.item.isFavorite
            QbzLibrary.libraryToggleFavorite("track", root.item.id)
        }
    }
}
