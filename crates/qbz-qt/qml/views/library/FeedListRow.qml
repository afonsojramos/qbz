// FeedListRow — the Library "All" feed's LIST row (FavoritesView.slint:1212+,
// the windowed mixed row: cover · title/subtitle · TYPE · source glyph ·
// quality · favourite indicator · ⋯).
//
// EXTRACTED from views/LibraryView.qml, where it was an inline `component`
// ~300 lines long inside a 1,880-line file (track rule 2). Behaviour is
// unchanged; the only edit is that the six things it used to read off `root`
// now come through `view` — the LibraryView instance — which is the same seam
// views/local/*.qml uses for LocalLibraryView.

import QtQuick
import com.blitzfc.qbz
import "../../cards"
import "../../controls"
import "../../theme"

Rectangle {
    id: feedRow

    /// The LibraryView root: artMap, skelPhase, showLocal and the three
    /// shared play/menu helpers live there (never duplicated per row).
    property var view: null
    property var item: ({})
    /// Viewport-relative position, for the skeleton's animated cap.
    property int rowIndex: 0

    QbzTheme { id: theme }

    height: 44
    radius: 6
    color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

    // Live heart, as a real property — `item.isFavorite = !…` notified
    // nothing (plain JS object; see rows/TrackRow.qml for the measurement)
    // so this row's glyph and its menu label never moved on click. The
    // binding is re-established on every new row object, so a republished
    // feed still wins.
    property bool favorite: feedRow.item.isFavorite === true
    onItemChanged: feedRow.favorite = Qt.binding(function () {
        return feedRow.item.isFavorite === true
    })
    function toggleFavorite() {
        feedRow.favorite = !feedRow.favorite
        QbzLibrary.libraryToggleFavorite(feedRow.item.kind, feedRow.item.id)
    }
    // Settle + rollback + cross-surface walk. `artKey` IS
    // `library_qt::feed_key(kind, id)`, the very key the signal carries —
    // which is also why LibraryView's own handler can patch the backing row by
    // comparing against it.
    Connections {
        target: QbzLibrary
        function onLibraryFavoriteChanged(key, value) {
            if ((feedRow.item.artKey || "") !== "" && key === feedRow.item.artKey)
                feedRow.favorite = value
        }
    }

    // Row body — click plays/opens by kind. Declared BEFORE the cells so
    // the ⋯ button and art-play win their clicks.
    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            if (feedRow.item.kind === "track") feedRow.view.playTrackInContext(feedRow.item.id)
            else if (feedRow.item.kind === "album") QbzAlbum.openAlbum(feedRow.item.id)
            else if (feedRow.item.kind === "artist") QbzArtist.openArtist(feedRow.item.id)
            else if (feedRow.item.kind === "playlist") QbzBridge.openPlaylist(feedRow.item.id)
            else if (feedRow.item.kind === "label") QbzHome.openLabel(feedRow.item.id)
        }
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: 12
        // Col — artwork (round for artists) + hover play.
        Rectangle {
            width: 44
            height: 44
            anchors.verticalCenter: parent.verticalCenter
            radius: feedRow.item.kind === "artist" ? 22 : 6
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                visible: !lrCollage.visible
                source: feedRow.view.artMap[feedRow.item.artKey] || ""
                radius: feedRow.item.kind === "artist" ? 22 : 6
                // A label logo is contain-fitted (cropping cuts the
                // wordmark) on a FLAT surface — those are transparent
                // PNGs whose edges carry no colour to derive from.
                // A playlist takes "auto": its own graphic is the 800x380
                // `image_rectangle` and pads with an image-derived
                // gradient, while a square member cover still crops. The
                // flag is not required for that any more, but it stays
                // authoritative where it IS published (this feed).
                fit: feedRow.item.kind === "label" ? "contain"
                    : feedRow.item.kind === "playlist"
                        ? (feedRow.item.playlistOwnImage === true ? "pad" : "auto")
                        : "crop"
            }
            // User playlists have no graphic of their own, so they show the
            // member-cover mosaic instead of a blank tile.
            PlaylistCollage {
                id: lrCollage
                anchors.fill: parent
                visible: feedRow.item.kind === "playlist"
                    && feedRow.item.playlistOwnImage !== true
                    && (feedRow.view.artMap[feedRow.item.artKey] || "") === ""
                urls: feedRow.item.covers || []
                radius: 6
            }
            // Per-row cover placeholder — same progressive rule as the
            // grid: it clears when THIS row's cover lands. Skipped for
            // the collage arm (a playlist mosaic is real content) and
            // for artists/labels (round designed placeholders).
            QbzSkeleton {
                variant: "art"
                anchors.fill: parent
                blockRadius: 6
                visible: (feedRow.item.kind === "track" || feedRow.item.kind === "album")
                    && feedRow.item.imageUrl !== ""
                    && (feedRow.view.artMap[feedRow.item.artKey] || "") === ""
                phase: feedRow.view.skelPhase
                cellIndex: feedRow.rowIndex
            }
            Rectangle {
                visible: feedRow.item.kind === "track" || feedRow.item.kind === "album"
                    || feedRow.item.kind === "playlist"
                anchors.fill: parent
                radius: feedRow.item.kind === "artist" ? 22 : 6
                color: "#a6000000"
                opacity: lrPlayArea.containsMouse ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                // On the #a6000000 artwork scrim — dark under every theme.
                QbzIcon { name: "play-fill"; width: 16; height: 16; anchors.centerIn: parent; tintName: "white" }
                MouseArea {
                    id: lrPlayArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (feedRow.item.kind === "track") feedRow.view.playTrackInContext(feedRow.item.id)
                        else if (feedRow.item.kind === "album") QbzPlayer.playAlbum(feedRow.item.id)
                        // `playPlaylistById`, NOT `playPlaylist` — the latter
                        // plays the OPEN playlist page and takes no id
                        // (player_bridge.rs:189 is the by-id entry).
                        else if (feedRow.item.kind === "playlist") QbzPlayer.playPlaylistById(feedRow.item.id)
                    }
                }
            }
        }
        // Col — title + subtitle (artist link for track/album).
        Column {
            width: parent.width - 44 - 116 - (srcCol.visible ? 44 : 0)
                - 150 - 18 - 30 - 6 * 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                height: 18
                text: feedRow.item.title
                color: lrTitleArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: 14
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                MouseArea {
                    id: lrTitleArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (feedRow.item.kind === "track") feedRow.view.playTrackInContext(feedRow.item.id)
                        else if (feedRow.item.kind === "album") QbzAlbum.openAlbum(feedRow.item.id)
                        else if (feedRow.item.kind === "artist") QbzArtist.openArtist(feedRow.item.id)
                        else if (feedRow.item.kind === "playlist") QbzBridge.openPlaylist(feedRow.item.id)
                        else if (feedRow.item.kind === "label") QbzHome.openLabel(feedRow.item.id)
                    }
                }
            }
            Text {
                visible: feedRow.item.subtitle !== ""
                width: parent.width
                height: 16
                text: feedRow.item.subtitle
                color: (feedRow.item.artistId !== ""
                        && (feedRow.item.kind === "track" || feedRow.item.kind === "album")
                        && lrSubArea.containsMouse) ? theme.accent : theme.textMuted
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                MouseArea {
                    id: lrSubArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: (feedRow.item.artistId !== ""
                                  && (feedRow.item.kind === "track" || feedRow.item.kind === "album"))
                        ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: if (feedRow.item.artistId !== "") QbzArtist.openArtist(feedRow.item.artistId)
                }
            }
        }
        // Col — type (icon + caps label).
        Rectangle {
            width: 116
            height: parent.height
            color: "transparent"
            Row {
                spacing: 6
                anchors.verticalCenter: parent.verticalCenter
                QbzIcon {
                    name: feedRow.item.kind === "track" ? "music"
                        : feedRow.item.kind === "playlist" ? "list-music"
                        : feedRow.item.kind === "artist" ? "user"
                        : feedRow.item.kind === "label" ? "disc-3" : "disc"
                    width: 13
                    height: 13
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: "muted"
                }
                Text {
                    text: feedRow.item.kind === "track" ? QbzSession.tr("Track", QbzSession.trRev)
                        : feedRow.item.kind === "album" ? QbzSession.tr("Album", QbzSession.trRev)
                        : feedRow.item.kind === "artist" ? QbzSession.tr("Artist", QbzSession.trRev)
                        : feedRow.item.kind === "playlist" ? QbzSession.tr("Playlist", QbzSession.trRev)
                        : QbzSession.tr("Label", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: 10
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 1.2
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
        // Col — source glyph (only when local/Plex can appear).
        Rectangle {
            id: srcCol
            visible: feedRow.view.showLocal
            width: 44
            height: parent.height
            color: "transparent"
            // Through controls/SourceIcon.qml, never QbzIcon: the Plex and
            // Qobuz marks are MULTI-COLOUR and a tint flattens them to a
            // silhouette. This column used to draw an accent-tinted
            // `hard-drive` for Plex — a blue hard drive.
            //
            // DEVIATION, stated on purpose: the .slint has NO source column in
            // this list at all (FavoritesView.slint:1767/2154 pass
            // `show-source: false` to AlbumCollectionView). The numbers below
            // are therefore its siblings' — AlbumListRow.slint:319, the row
            // glyph, not the card badge.
            SourceIcon {
                visible: (feedRow.item.source || "") !== ""
                    && feedRow.item.source !== "qobuz"
                kind: feedRow.item.source || ""
                // A dense ROW: the media marks draw monochrome and tinted, like the
                // hard-drive beside them. Colour logos are for cards — a list of
                // them fights the text it labels.
                mono: true
                glyphSize: 15
                plexSize: 16
                qobuzSize: 16
                // Host is a transparent column over the theme row (NOT an
                // artwork scrim), so this matches its siblings
                // local/LocalAlbumRow.qml and local/LocalTrackRow.qml.
                localTint: "muted"
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        // Col — quality (albums + tracks).
        Rectangle {
            width: 150
            height: parent.height
            color: "transparent"
            QualityMini {
                visible: (feedRow.item.kind === "album" || feedRow.item.kind === "track")
                    && feedRow.item.qualityTier !== ""
                tier: feedRow.item.qualityTier
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        // Col — favorite / follow indicator.
        QbzIcon {
            name: feedRow.item.kind === "artist" ? "user-plus"
                : (feedRow.favorite ? "heart-filled" : "heart")
            width: 18
            height: 18
            anchors.verticalCenter: parent.verticalCenter
            tintName: "accent"
        }
        // Col — context-menu button.
        Rectangle {
            width: 30
            height: 30
            radius: 6
            anchors.verticalCenter: parent.verticalCenter
            color: lrMenuArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon { name: "ellipsis"; width: 16; height: 16; anchors.centerIn: parent; tintName: "secondary" }
            MouseArea {
                id: lrMenuArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: function (mouse) { feedRow.openLrMenu(lrMenuArea, mouse.x, mouse.y) }
            }
        }
    }
    // LAZY. This is a QtQuick.Controls Popup with a Repeater over its
    // entries, and it was built EAGERLY for every row — so a long list
    // constructed one whole popup per visible row, and rebuilt them all
    // on every scroll step, to show a menu only ever opened on click.
    // Same lazy-Loader idiom this file already uses for the clipboard
    // helper and the info modal.
    Loader {
        id: lrMenuLoader
        active: false
        sourceComponent: CardMenu {
            menuWidth: 196
            entries: feedRow.item.kind === "track"
                ? feedRow.view.trackMenuModel(feedRow.item, feedRow.favorite)
                : feedRow.item.kind === "album" ? [
                    { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
                    { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                    { "label": feedRow.favorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                      "icon": feedRow.favorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
                : feedRow.item.kind === "artist" ? [
                    { "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" },
                ]
                : [
                    { "label": QbzSession.tr("Open", QbzSession.trRev), "icon": "list-music", "action": "open" },
                    { "label": feedRow.favorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                      "icon": feedRow.favorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
            onPicked: function (a) {
                if (feedRow.item.kind === "track") { feedRow.view.trackAction(feedRow, a); return }
                if (a === "open") {
                    if (feedRow.item.kind === "album") QbzAlbum.openAlbum(feedRow.item.id)
                    else if (feedRow.item.kind === "playlist") QbzBridge.openPlaylist(feedRow.item.id)
                    else if (feedRow.item.kind === "label") QbzHome.openLabel(feedRow.item.id)
                } else if (a === "play") {
                    if (feedRow.item.kind === "album") QbzPlayer.playAlbum(feedRow.item.id)
                } else if (a === "go-artist") {
                    QbzArtist.openArtist(feedRow.item.id)
                } else if (a === "favorite") {
                    feedRow.toggleFavorite()
                }
            }
        }
    }
    /// Build the popup on first use, then open it.
    function openLrMenu(anchor, x, y) {
        lrMenuLoader.active = true
        lrMenuLoader.item.openAtCursor(anchor, x, y)
    }
}
