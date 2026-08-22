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

    // --- Heart state (a real QML property, never `item.isFavorite`) ------
    // Identical reasoning to rows/TrackRow.qml: `item` is a plain JS object
    // copied out of `modelData`, so mutating a field on it fires no notifier
    // (the glyph never changed) and reaches no model. The binding is
    // re-established on every new row object so the document stays
    // authoritative.
    property bool favorite: root.item.isFavorite === true
    onItemChanged: root.favorite = Qt.binding(function () {
        return root.item.isFavorite === true
    })

    function toggleFavorite() {
        root.favorite = !root.favorite
        QbzLibrary.libraryToggleFavorite("track", root.item.id)
    }

    function openMenu(anchor, x, y) {
        trackMenuLoader.active = true
        trackMenuLoader.item.openAtCursor(anchor, x, y)
    }

    // Fan-out + rollback (the shape cards/AlbumCard.qml uses for pin).
    Connections {
        target: QbzLibrary
        function onLibraryFavoriteChanged(key, value) {
            var tid = (root.item && root.item.id !== undefined) ? root.item.id : ""
            if (tid !== "" && key === "track:" + tid)
                root.favorite = value
        }
    }

    Column {
        spacing: 0
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            // No clip: the child RoundedImage confines itself on both arms, and
            // a rectangular scissor never produced this radius. A clip is an
            // unconditional batch root, so this one cost a draw call per item.
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
            // Body click PLAYS the track (TrackCard hover); right press opens
            // the SAME menu as the ⋯ button.
            MouseArea {
                id: tcArtArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: function (mouse) {
                    if (mouse.button === Qt.RightButton)
                        root.openMenu(tcArtArea, mouse.x, mouse.y)
                    else
                        QbzPlayer.playTrack(root.item.id)
                }
            }
            // Hover overlay — favorite / play / more (y=120, h=44,
            // centered, spacing 12).
            CardOverlayRow {
                y: 120
                width: parent.width
                shown: root.overlayOn
                CardOverlayButton {
                    id: favBtn
                    name: root.favorite ? "heart-filled" : "heart"
                    active: root.favorite
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.toggleFavorite()
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
                    onClicked: function (mouse) { root.openMenu(moreBtn, mouse.x, mouse.y) }
                }
            }
            // Source badge (All feed, show-local): bottom-right of the art.
            // 24x24 rounded SQUARE (ADR-008: not a pill), 6px inset —
            // discover/TrackCard.slint:177-197. Simpler than the album card's:
            // ONE chip colour (#000000b3 -> Qt #b3000000) and no purchase arm.
            //
            // The glyph goes through controls/SourceIcon.qml, never QbzIcon:
            // the Plex and Qobuz marks are MULTI-COLOUR and a tint flattens
            // them to a silhouette (this card used to draw an accent-tinted
            // `hard-drive` for Plex — a blue hard drive).
            Rectangle {
                visible: root.showSourceBadge && (root.item.source || "") !== ""
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                color: "#b3000000"
                SourceIcon {
                    // `sourceRaw` FIRST (contract §D.1): local rows carry
                    // `qobuz_purchase` there, and `source` folds it into
                    // "offline" for the source chips. Feed rows publish no
                    // `sourceRaw` at all, so they fall through unchanged.
                    kind: root.item.sourceRaw || root.item.source || ""
                    // .slint:191-193 — plex 16, qobuz 18, local 14. "offline"
                    // is the Qt word for a Qobuz download (local_rows.rs
                    // `badge_source`); the .slint's local-library track rows
                    // reach the same arm spelled "qobuz" (local_library.rs:1047).
                    glyphSize: 14
                    plexSize: 16
                    qobuzSize: 18
                    // .slint:190 — #ffffff on the near-black chip.
                    localTint: "white"
                    // Explicit pixel rounding (.slint:194-195).
                    x: Math.round((parent.width - width) / 2)
                    y: Math.round((parent.height - height) / 2)
                }
            }
            Loader {
                id: trackMenuLoader
                active: false
                sourceComponent: CardMenu {
                    menuWidth: 196
                    entries: root.menuModel()
                    onPicked: function (a) { root.trackAction(a) }
                }
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
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: function (mouse) {
                            if (mouse.button === Qt.RightButton)
                                root.openMenu(tcTitleArea, mouse.x, mouse.y)
                            else
                                QbzPlayer.playTrack(root.item.id)
                        }
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
    // COMPLETE against the .slint: Play · Play next · Play later (#442, end
    // of the manual block) · Add to queue · Go to artist (artist-id != "") ·
    // Go to album (album-id != "") · favorite. The card menu is deliberately
    // shorter than the row menu — the .slint card has no radio / share /
    // offline / track-info rows.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": t("Play", r), "icon": "play-fill", "action": "play" },
            { "label": t("Play next", r), "icon": "list-start", "action": "next" },
            { "label": t("Play later", r), "icon": "list-plus", "action": "later" },
            { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
        ]
        if (root.item.artistId) m.push({ "label": t("Go to artist", r), "icon": "user", "action": "go-artist" })
        if (root.item.albumId) m.push({ "label": t("Go to album", r), "icon": "disc", "action": "go-album" })
        m.push({ "label": root.favorite ? t("Remove from Library", r) : t("Add to Library", r),
                 "icon": root.favorite ? "heart-filled" : "heart", "action": "favorite" })
        return m
    }
    function trackAction(a) {
        if (a === "play") QbzPlayer.playTrack(root.item.id)
        else if (a === "next") QbzPlayer.enqueueTrack(root.item.id, "next")
        else if (a === "later") QbzPlayer.enqueueTrack(root.item.id, "later")
        else if (a === "queue") QbzPlayer.enqueueTrack(root.item.id, "queue")
        else if (a === "go-artist") QbzArtist.openArtist(root.item.artistId)
        else if (a === "go-album") QbzAlbum.openAlbum(root.item.albumId)
        else if (a === "favorite") root.toggleFavorite()
    }
}
