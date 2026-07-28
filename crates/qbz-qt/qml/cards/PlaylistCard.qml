// PlaylistCard — THE playlist card (discover/PlaylistCard.slint), shared
// by every surface since phase 21: Home/Editor/ForYou playlist rails, the
// Pinned rail, the Library playlist grid + All feed, the Search all-tab
// carousel + playlists grid.
//
// 200x246: 200px cover (Radius.sm) + hover scrim 0.6, overlay row at
// y=120 (fav tri-state / play / more), pin badge top-right, then the
// left-aligned meta: semibold title + (category accent eyebrow | muted
// subtitle). Body click OPENS the playlist (the Slint body-opens arm —
// every Slint mount sets it).
//
// item contract: { id, title, subtitle, category, playlistOwned,
// playlistFollowing, isFavorite } plus the scalar artSource / artworkUrl
// / isPinned props (the AlbumCard pattern).
//
// Ownership tri-state (Slint): owned -> heart (library favorite);
// foreign followed -> check; foreign -> user-plus (Qobuz follow).

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
    // Pinned state (AlbumCard pattern: scalar prop, optimistic flip on
    // click — the model re-publish re-creates the delegate).
    property bool isPinned: false

    color: "transparent"

    QbzTheme { id: theme }

    readonly property bool overlayOn: plArtArea.containsMouse || pinArea.containsMouse
        || plFav.hovered || plPlay.hovered || plMore.hovered

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
            MouseArea {
                id: plArtArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // body-opens (every Slint mount).
                onClicked: QbzBridge.openPlaylist(root.item.id)
            }
            // Hover overlay — fav / play / more (y=120, h=44, centered).
            Row {
                y: 120
                width: 200
                height: 44
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                CardOverlayButton {
                    id: plFav
                    name: root.item.playlistOwned ? "heart"
                        : root.item.playlistFollowing ? "check" : "user-plus"
                    active: root.item.playlistOwned === true && root.item.isFavorite === true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: {
                        if (root.item.playlistOwned) {
                            root.item.isFavorite = !root.item.isFavorite
                            QbzBridge.libraryToggleFavorite("playlist", root.item.id)
                        } else {
                            root.item.playlistFollowing = !root.item.playlistFollowing
                            QbzBridge.playlistSetFollowById(root.item.id, root.item.playlistFollowing)
                        }
                    }
                }
                CardOverlayButton {
                    id: plPlay
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: QbzPlayer.playPlaylistById(root.item.id)
                }
                CardOverlayButton {
                    id: plMore
                    name: "ellipsis"
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: function (mouse) { plMenu.openAtCursor(plMore, mouse.x, mouse.y) }
                }
                Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
            }
            // Pin badge — top-right (opacity follows overlay-on).
            Rectangle {
                x: parent.width - width - 8
                y: 8
                width: 26
                height: 26
                radius: 13
                color: pinArea.containsMouse ? "#cc000000" : "#99000000"
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                QbzIcon {
                    name: root.isPinned ? "pin-filled" : "pin"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: root.isPinned ? "accent" : "primary"
                }
                MouseArea {
                    id: pinArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.isPinned = !root.isPinned
                        QbzBridge.togglePin("playlist", root.item.id, root.item.title,
                            root.item.subtitle || "", root.artworkUrl)
                    }
                }
            }
            CardMenu {
                id: plMenu
                menuWidth: 196
                // PlaylistCard.slint's menu: queueing actions + favorite
                // (owned) / follow (foreign).
                entries: [
                    { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                    { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "play-next" },
                    { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "play-later" },
                    { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                    { "label": root.item.playlistOwned
                        ? (root.item.isFavorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev))
                        : (root.item.playlistFollowing ? QbzSession.tr("Unfollow on Qobuz", QbzSession.trRev) : QbzSession.tr("Follow on Qobuz", QbzSession.trRev)),
                      "icon": root.item.playlistOwned ? (root.item.isFavorite ? "heart-filled" : "heart")
                          : (root.item.playlistFollowing ? "check" : "user-plus"),
                      "action": "favorite" },
                ]
                onPicked: function (a) {
                    if (a === "play") QbzPlayer.playPlaylistById(root.item.id)
                    else if (a === "play-next") QbzPlayer.enqueuePlaylistById(root.item.id, "next")
                    else if (a === "play-later") QbzPlayer.enqueuePlaylistById(root.item.id, "later")
                    else if (a === "queue") QbzPlayer.enqueuePlaylistById(root.item.id, "queue")
                    else if (a === "favorite") {
                        if (root.item.playlistOwned) {
                            root.item.isFavorite = !root.item.isFavorite
                            QbzBridge.libraryToggleFavorite("playlist", root.item.id)
                        } else {
                            root.item.playlistFollowing = !root.item.playlistFollowing
                            QbzBridge.playlistSetFollowById(root.item.id, root.item.playlistFollowing)
                        }
                    }
                }
            }
        }
        Item { width: 1; height: 6 }
        // Meta: left-aligned, semibold title + (category eyebrow | subtitle).
        Column {
            width: 200
            height: 40
            spacing: 2
            leftPadding: theme.spacingXs
            Text {
                width: parent.width
                height: 20
                text: root.item.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightSemibold
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                visible: !!root.item.category
                width: parent.width
                height: 16
                text: (root.item.category || "").toUpperCase()
                color: theme.accent
                font.pixelSize: 11
                font.weight: theme.weightSemibold
                font.letterSpacing: 0.5
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                visible: !root.item.category
                width: parent.width
                height: 16
                text: root.item.subtitle || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLink - 1
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
    }
}
