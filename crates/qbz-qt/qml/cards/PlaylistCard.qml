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
// Artwork, two arms (1:1 with Tauri, which has TWO playlist cards):
//   - `item.playlistOwnImage` — the playlist carries its OWN Qobuz graphic
//     (`image_rectangle`). Those are landscape and cropping butchers them,
//     so they render CONTAIN (QobuzPlaylistCard: `object-fit: contain`).
//   - otherwise — the mosaic of member-track covers from `item.covers`
//     (FavoritePlaylistCard -> PlaylistCollage). Rows without either get
//     the collage's list-music placeholder.
// Surfaces that publish neither field (Home rails, Search) keep the single
// crop-fitted cover they already passed through `artSource`.
//
// Ownership tri-state (Slint): owned -> heart (library favorite);
// foreign followed -> check; foreign -> user-plus (Qobuz follow).
//
// --- Menu inventory vs discover/PlaylistCard.slint ---------------------
//   Play · Play next · Play later · Add to queue ·
//   Add to Library (is-owned) | Follow/Unfollow on Qobuz (!is-owned) ·
//   Copy to your library (!is-owned && !is-copied)
// All live except the last — see `hasCopyByIdSeam`. The ⋯ button and a
// right press on the cover or the title open the same menu.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    // Host-resolved artwork path (the AlbumCard artSource pattern).
    property string artSource: ""
    // Remote cover URL for the pin payload. Defaults to whatever the row
    // carries — the playlist's own graphic, else its first member cover —
    // so a pinned playlist keeps art in the Pinned rail; hosts that resolve
    // it themselves may still override.
    property string artworkUrl: (root.item && root.item.imageUrl)
        ? root.item.imageUrl
        : (root.collageUrls.length > 0 ? root.collageUrls[0] : "")

    // Member-track covers for the mosaic arm ([] on surfaces that publish a
    // single pre-resolved cover instead).
    readonly property var collageUrls: (root.item && root.item.covers)
        ? root.item.covers : []
    // The row's image is the playlist's own Qobuz graphic -> contain.
    readonly property bool ownImage: root.item
        ? root.item.playlistOwnImage === true : false
    // Pinned state (AlbumCard pattern: scalar prop, optimistic flip on
    // click — the model re-publish re-creates the delegate).
    property bool isPinned: false

    // "Copy to your library": QbzBridge.playlistCopy() copies the playlist
    // that is currently OPEN — it takes no id, so a card cannot call it. The
    // row stays out of the menu until a by-id copy invokable exists (and
    // `item.playlistCopied` is published for the .slint's is-copied gate).
    readonly property bool hasCopyByIdSeam: false

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
            // Arm 1 — a resolved single cover. Contain for the playlist's
            // own (landscape) Qobuz graphic, crop for everything else.
            RoundedImage {
                anchors.fill: parent
                visible: root.artSource !== ""
                source: root.artSource
                radius: theme.radiusSm
                fit: root.ownImage ? "contain" : "crop"
            }
            // Arm 2 — the member-cover mosaic. A row that HAS its own
            // graphic passes no urls, so it shows the placeholder while
            // that graphic downloads instead of flashing a mosaic.
            PlaylistCollage {
                anchors.fill: parent
                visible: root.artSource === ""
                urls: root.ownImage ? [] : root.collageUrls
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
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // body-opens (every Slint mount); right press = the ⋯ menu.
                onClicked: function (mouse) {
                    if (mouse.button === Qt.RightButton)
                        plMenu.openAtCursor(plArtArea, mouse.x, mouse.y)
                    else
                        QbzBridge.openPlaylist(root.item.id)
                }
            }
            // Hover overlay — fav / play / more (y=120, h=44, centered).
            CardOverlayRow {
                y: 120
                width: parent.width
                shown: root.overlayOn
                CardOverlayButton {
                    id: plFav
                    name: root.item.playlistOwned ? "heart"
                        : root.item.playlistFollowing ? "check" : "user-plus"
                    active: root.item.playlistOwned === true && root.item.isFavorite === true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: {
                        if (root.item.playlistOwned) {
                            root.item.isFavorite = !root.item.isFavorite
                            QbzLibrary.libraryToggleFavorite("playlist", root.item.id)
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
                    // On a #99000000/#cc000000 scrim — dark under every theme.
                    tintName: root.isPinned ? "accent" : "white"
                }
                MouseArea {
                    id: pinArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.isPinned = !root.isPinned
                        QbzLibrary.togglePin("playlist", root.item.id, root.item.title,
                            root.item.subtitle || "", root.artworkUrl)
                    }
                }
            }
            CardMenu {
                id: plMenu
                menuWidth: 196
                entries: root.menuModel()
                onPicked: function (a) { root.menuAction(a) }
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

    // Right-press catcher over the meta block (the cover has its own in
    // plArtArea). Declared OUTSIDE the Column so it takes no layout space,
    // and RightButton-only so left clicks still fall through to the text.
    MouseArea {
        id: plMetaArea
        x: 0
        y: 206
        width: 200
        height: 40
        acceptedButtons: Qt.RightButton
        onClicked: function (mouse) { plMenu.openAtCursor(plMetaArea, mouse.x, mouse.y) }
    }

    // PlaylistCard.slint's menu, in its order.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var owned = root.item.playlistOwned === true
        var following = root.item.playlistFollowing === true
        var m = [
            { "label": t("Play", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "play-next" },
            // #442 "Play later" — end of the manual block.
            { "label": t("Play later", r), "icon": "list-plus", "action": "play-later" },
            { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
        ]
        if (owned)
            m.push({ "label": root.item.isFavorite ? t("Remove from Library", r) : t("Add to Library", r),
                     "icon": root.item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
        else
            m.push({ "label": following ? t("Unfollow on Qobuz", r) : t("Follow on Qobuz", r),
                     "icon": following ? "check" : "user-plus", "action": "favorite" })
        if (root.hasCopyByIdSeam && !owned && root.item.playlistCopied !== true)
            m.push({ "label": t("Copy to your library", r), "icon": "copy", "action": "copy" })
        return m
    }

    function menuAction(a) {
        if (a === "play") QbzPlayer.playPlaylistById(root.item.id)
        else if (a === "play-next") QbzPlayer.enqueuePlaylistById(root.item.id, "next")
        else if (a === "play-later") QbzPlayer.enqueuePlaylistById(root.item.id, "later")
        else if (a === "queue") QbzPlayer.enqueuePlaylistById(root.item.id, "queue")
        else if (a === "favorite") {
            if (root.item.playlistOwned) {
                root.item.isFavorite = !root.item.isFavorite
                QbzLibrary.libraryToggleFavorite("playlist", root.item.id)
            } else {
                root.item.playlistFollowing = !root.item.playlistFollowing
                QbzBridge.playlistSetFollowById(root.item.id, root.item.playlistFollowing)
            }
        }
    }
}
