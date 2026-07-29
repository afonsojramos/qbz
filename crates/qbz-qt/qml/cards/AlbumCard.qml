// THE shared album card — QML replica of discover/AlbumCard.slint, used
// by BOTH the Home rails and the Library All grid (the Slint mounts the
// same component in both places).
//
// 200x246: 200px artwork (Radius.sm) + placeholder, hover scrim with
// genre/year meta, pin badge (top-right), favorite / play / more overlay
// buttons, award ribbon, source badge (opt-in), then the title/artist
// lines with the icon-only quality badge.
//
// Live wiring: play (art click + overlay play), favorite heart (optimistic
// + signal), pin badge (pinned store), ⋯ context menu — and the SAME menu
// on a right press anywhere on the artwork or the title.
//
// --- Menu inventory vs discover/AlbumCard.slint ------------------------
//   Open album · Play · Play next · Play later · Add to queue ·
//   Add to/Remove from Library (show-favorite) · Block this album
//     (source != local && source != plex)
// All but the last are live. "Block this album" is gated OFF by
// `hasBlacklistSeam` below: the Qt bridge exposes the blacklist COUNTERS
// (settings_qt/devtools.rs) but no block-album invokable, and the row used
// to render and do nothing at all.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    // --- card data (the ONE contract both hosts fill) --------------------
    property string albumId: ""
    property string title: ""
    property string artist: ""
    property string artistId: ""
    property string genre: ""
    property string year: ""
    property string qualityTier: ""
    property string ribbon: ""
    property string ribbonKind: ""
    // Artwork image source (file://… or "") — the host's cache lookup.
    property string artSource: ""
    // REMOTE cover url, for the pin payload only (the AlbumCard.slint pin
    // TouchArea passes `album.artwork-url`). The pinned store keeps a
    // denormalized display snapshot taken at pin time, so a host that
    // leaves this empty pins a row the Pinned rail can only draw as a
    // placeholder — `artSource` is NOT a substitute, it is a local
    // file:// path that means nothing on another machine or after a
    // cache wipe.
    property string artworkUrl: ""
    property bool isFavorite: false
    property bool isPinned: false
    // Source badge (Library show-local): "local" | "plex" | "" (hidden).
    property string source: ""
    // Local play count — rendered as the muted "{} plays" line under the
    // artist, and ONLY there (discover/AlbumCard.slint:508-513). Every
    // surface but Most Played Albums publishes 0, which is why that page's
    // card is 286px tall and every other one is 266px: this line IS the
    // 20px difference.
    property int plays: 0

    // --- Local Library mode (additive; default = the Qobuz behaviour) ----
    // The Local Library mounts THIS card, but its `albumId` is a group key
    // (folder or metadata identity), not a Qobuz catalog id — routing it
    // through QbzAlbum.openAlbum / QbzPlayer.playAlbum would fire a
    // catalog fetch for a folder path. In localMode every action is emitted
    // to the host instead, and the catalog-only affordances (heart, pin,
    // "Block this album") are hidden. Nothing else about the card changes,
    // so the two surfaces stay pixel-identical.
    property bool localMode: false

    // Album blacklist ("Block this album"): no write seam on the Qt bridge —
    // the entry stays out of the menu until one lands, rather than shipping
    // a row that silently no-ops. Flip this and fill the "block" branch in
    // `menuAction` together.
    readonly property bool hasBlacklistSeam: false

    signal openRequested()
    signal playRequested()
    signal enqueueRequested(string mode)

    QbzTheme { id: theme }

    width: 200
    // 246 normally; +20 for the "{} plays" line (see `plays`) — the same
    // 266 -> 286 step MostPlayedAlbumsView.slint:23 takes for its grid.
    height: 246 + (root.plays > 0 ? 20 : 0)
    color: "transparent"

    readonly property bool overlayOn: artArea.containsMouse || pinArea.containsMouse
        || favBtn.hovered || playBtn.hovered || moreBtn.hovered

    function toggleFavorite() {
        root.isFavorite = !root.isFavorite
        QbzLibrary.libraryToggleFavorite("album", root.albumId)
    }
    function togglePin() {
        root.isPinned = !root.isPinned
        QbzLibrary.togglePin("album", root.albumId, root.title, root.artist,
            root.artworkUrl)
    }

    // Pin fan-out. The pinned store has no change-notify, so `pinChanged`
    // (key `{kind}:{id}`, emitted by main.rs::toggle_pin after the write) is
    // the ONLY signal that this album's state moved — and it moves from
    // anywhere: another rail, another tab, the album page header. This is the
    // port's equivalent of the Slint `set_album_row_pinned` walk over every
    // live model, and the reason no surface has to republish its document to
    // keep a glyph honest. Assigning breaks the host's `isPinned` binding on
    // purpose (the optimistic flip above already does).
    Connections {
        target: QbzLibrary
        function onPinChanged(key, value) {
            if (root.albumId !== "" && key === "album:" + root.albumId)
                root.isPinned = value
        }
        // Favourite fan-out — the SAME shape, one signal later. `isFavorite`
        // was the odd one out: the optimistic flip above breaks the host's
        // binding (by design), and nothing settled it afterwards, so a write
        // that FAILED stayed visibly wrong until the user navigated away, and
        // a heart flipped on another surface (the album page header, a search
        // result, the queue) never reached this card. `libraryFavoriteChanged`
        // carries the value the write actually produced — the flipped one on
        // success, the UNCHANGED one on failure — so this is both the
        // cross-surface walk and the rollback.
        function onLibraryFavoriteChanged(key, value) {
            if (root.albumId !== "" && key === "album:" + root.albumId)
                root.isFavorite = value
        }
    }

    // AlbumCard.slint's album-menu, in its order. localMode drops the
    // catalog-only rows (heart + block); the five navigation/playback rows
    // are identical and route to the host's signals instead.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": t("Open album", r), "icon": "library-big", "action": "open" },
            { "label": t("Play", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "next" },
            // #442 "Play later" — end of the manual block.
            { "label": t("Play later", r), "icon": "list-plus", "action": "later" },
            { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
        ]
        if (!root.localMode) {
            m.push({ "label": root.isFavorite ? t("Remove from Library", r) : t("Add to Library", r),
                     "icon": root.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
            // .slint gates this on a non-local/plex source as well.
            if (root.hasBlacklistSeam && root.source !== "local" && root.source !== "plex")
                m.push({ "label": t("Block this album", r), "icon": "blind-eye", "action": "block" })
        }
        return m
    }

    function menuAction(a) {
        if (root.localMode) {
            if (a === "open") root.openRequested()
            else if (a === "play") root.playRequested()
            else if (a === "next") root.enqueueRequested("next")
            else if (a === "later") root.enqueueRequested("later")
            else if (a === "queue") root.enqueueRequested("queue")
            return
        }
        if (a === "open") QbzAlbum.openAlbum(root.albumId)
        else if (a === "play") QbzPlayer.playAlbum(root.albumId)
        else if (a === "next") QbzPlayer.enqueueAlbum(root.albumId, "next")
        else if (a === "later") QbzPlayer.enqueueAlbum(root.albumId, "later")
        else if (a === "queue") QbzPlayer.enqueueAlbum(root.albumId, "queue")
        else if (a === "favorite") root.toggleFavorite()
    }

    Column {
        spacing: 0

        // --- Artwork + hover overlay -----------------------------------
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

            // Hover scrim.
            Rectangle {
                anchors.fill: parent
                radius: theme.radiusSm
                color: "#000000"
                opacity: root.overlayOn ? 0.6 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }

            // Hover meta — genre + year, top-left.
            Column {
                x: 12
                y: 12
                spacing: 2
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                Text {
                    visible: root.genre !== ""
                    text: root.genre
                    height: 20
                    color: "#ebffffff"
                    font.pixelSize: 13
                    font.weight: theme.weightBold
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    visible: root.year !== ""
                    text: root.year
                    height: 17
                    color: "#ccffffff"
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }
            }

            // Card-open + hover detector (declared before the action
            // buttons so those win the pointer).
            MouseArea {
                id: artArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Right press opens the SAME menu as the ⋯ button.
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                // Phase 8: the card opens the album view (the overlay play
                // button carries the play affordance).
                onClicked: function (mouse) {
                    if (mouse.button === Qt.RightButton) {
                        albumMenu.openAtCursor(artArea, mouse.x, mouse.y)
                        return
                    }
                    if (root.localMode)
                        root.openRequested()
                    else
                        QbzAlbum.openAlbum(root.albumId)
                }
            }

            // Pin badge — top-right. Hover-revealed like the overlay
            // buttons (AlbumCard.slint: opacity follows overlay-on even
            // when pinned — the pinned state reads in the icon swap only:
            // filled accent pin vs outline). Always-mounted (opacity) so
            // its hover joins overlayOn.
            Rectangle {
                visible: !root.localMode
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
                    onClicked: root.togglePin()
                }
            }

            // Hover action buttons — favorite / play / more (y=120, h=44,
            // centered, spacing 12).
            CardOverlayRow {
                y: 120
                width: parent.width
                shown: root.overlayOn

                CardOverlayButton {
                    id: favBtn
                    visible: !root.localMode
                    name: root.isFavorite ? "heart-filled" : "heart"
                    active: root.isFavorite
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.toggleFavorite()
                }
                CardOverlayButton {
                    id: playBtn
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.localMode ? root.playRequested()
                                              : QbzPlayer.playAlbum(root.albumId)
                }
                CardOverlayButton {
                    id: moreBtn
                    name: "ellipsis"
                    anchors.verticalCenter: parent.verticalCenter
                    // CardOverlayButton.clicked() carries no mouse payload,
                    // so fall back to the disc's centre — the menu still
                    // opens under the ⋯ (worst case 18px off the pointer).
                    // Stays correct if the signal ever forwards the event.
                    onClicked: function (mouse) {
                        albumMenu.openAtCursor(moreBtn,
                            mouse ? mouse.x : moreBtn.width / 2,
                            mouse ? mouse.y : moreBtn.height / 2)
                    }
                }
            }

            // Context menu (AlbumCard.slint's album-menu) — the shared
            // CardMenu surface, not a second copy of its delegate.
            CardMenu {
                id: albumMenu
                menuWidth: 196
                entries: root.menuModel()
                onPicked: function (a) { root.menuAction(a) }
            }

            // Award ribbon — content-width, capped at the card width.
            Rectangle {
                visible: root.ribbon !== ""
                x: 0
                y: parent.height - height - 8
                height: 20
                width: Math.min(ribbonRow.width, 200)
                color: root.ribbonKind === "press" ? "#d49511" : "#e0000000"
                topRightRadius: 3
                bottomRightRadius: 3
                clip: true
                Rectangle {
                    width: root.ribbonKind === "press" ? 0 : 3
                    height: parent.height
                    color: root.ribbonKind === "qobuzissime" ? "#8b5cf6" : "#eab308"
                }
                Row {
                    id: ribbonRow
                    height: parent.height
                    leftPadding: 10
                    rightPadding: 10
                    width: ribbonText.implicitWidth + 20
                    Text {
                        id: ribbonText
                        height: parent.height
                        text: root.ribbon
                        color: root.ribbonKind === "press" ? "#1f1407" : "#ffffff"
                        font.pixelSize: 9
                        font.weight: theme.weightSemibold
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
            }

            // Source badge (Library show-local): bottom-right of the art.
            Rectangle {
                visible: root.source === "local" || root.source === "plex"
                    || root.source === "offline"
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                color: "#b3000000"
                QbzIcon {
                    // The Local Library's third source: a Qobuz offline copy
                    // (LocalLibraryView.slint's `show-source-badge` triple).
                    name: root.source === "offline" ? "cloud-download" : "hard-drive"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    // On the #b3000000 badge — dark under every theme.
                    tintName: root.source === "plex" ? "accent" : "white"
                }
            }
        }
        Item { width: 1; height: 6 }

        // --- Title / artist + quality badge ------------------------------
        Row {
            width: 200
            height: 40 + (root.plays > 0 ? 20 : 0)
            spacing: theme.spacingSm
            Column {
                width: parent.width - (qBadge.visible ? qBadge.width + theme.spacingSm : 0)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 20
                    text: root.title
                    color: titleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: titleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function (mouse) {
                            if (mouse.button === Qt.RightButton) {
                                albumMenu.openAtCursor(titleArea, mouse.x, mouse.y)
                                return
                            }
                            if (root.localMode)
                                root.openRequested()
                            else
                                QbzAlbum.openAlbum(root.albumId)
                        }
                    }
                }
                Text {
                    width: parent.width
                    height: 18
                    text: root.artist
                    color: root.artistId !== "" && artistArea.containsMouse
                        ? theme.textPrimary : theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (root.artistId !== "") QbzArtist.openArtist(root.artistId)
                    }
                }
                // Play count — Most Played Albums only (see `plays`).
                Text {
                    visible: root.plays > 0
                    width: parent.width
                    height: visible ? 18 : 0
                    text: QbzSession.tr("{} plays", QbzSession.trRev).replace("{}", root.plays)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
            // Icon-only quality badge (QualityBadge.slint).
            QualityMini {
                id: qBadge
                tier: root.qualityTier
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
