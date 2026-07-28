// Search results view — the QML port of search/SearchResultsView.slint
// (phase 15). One JSON document (QbzBridge.searchJson, search_qt.rs
// SearchPageDoc: query/tab/loading/filterIndex + the four category lists,
// totals, the most-popular hero, the carousel-only artists list).
//
// Layout (1:1): 56px toolbar ("Search" title + the searchType filter
// radios on the right), the five-tab strip (All / Albums / Tracks /
// Artists / Playlists), the scrollable body: All = Most-popular hero +
// Artists carousel + Albums carousel + Tracks preview (6) + Playlists
// carousel; per-type tabs = full grid/list + "Load more (n / total)".
//
// POC-NOTEs:
// - Card click actions: albums/artists navigate; tracks play; playlists
//   open (openPlaylist is wired; the stale "inert" note was dropped in
//   phase 21, when every card surface moved to the shared qml/ cards).
// - The most-popular track hero stays a DISTINCT component
//   (SearchTrackHero below): in Slint the search hero is
//   primitives/SearchTrackHero.slint, NOT discover/TrackCard.slint —
//   the POC variant is 200x246 (centered play, quality as text) to match
//   the ArtistGridCard hero slot it shares the row with.
// - The Slint's windowed grid virtualization is not replicated (page size
//   is 20 — the whole set mounts).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../theme"

Rectangle {
    id: root
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzBridge.searchJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var albums: doc.albums || []
    readonly property var tracks: doc.tracks || []
    readonly property var artists: doc.artists || []
    readonly property var artistsCarousel: doc.artistsCarousel || []
    readonly property var playlists: doc.playlists || []
    readonly property var mp: doc.mostPopular || ({})
    readonly property bool loading: doc.loading === true
    readonly property int tab: doc.tab || 0
    readonly property int filterIndex: doc.filterIndex || 0
    readonly property bool hasResults: albums.length + tracks.length + artists.length + playlists.length > 0
    readonly property int previewCap: 6

    // Track hero (SearchTrackHero: the 200x246 track card + the quality
    // label as TEXT under the meta).
    component SearchTrackHero: Rectangle {
        property var card: ({})
        property string qualityLabel: ""
        color: "transparent"
        readonly property bool overlayOn: heroArtArea.containsMouse

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
                    source: card.artPath || ""
                    radius: theme.radiusSm
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: parent.parent.overlayOn ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                Rectangle {
                    anchors.centerIn: parent
                    width: 44
                    height: 44
                    radius: 22
                    opacity: parent.parent.overlayOn ? 1.0 : 0.0
                    color: heroPlayArea.containsMouse ? "#d6ffffff" : "#ffffff"
                    QbzIcon { name: "play-fill"; width: 18; height: 18; anchors.centerIn: parent; tintName: "black" }
                    MouseArea {
                        id: heroPlayArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzPlayer.playTrack(card.id)
                    }
                }
                MouseArea {
                    id: heroArtArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzPlayer.playTrack(card.id)
                }
            }
            Item { width: 1; height: 6 }
            Text {
                width: 200
                height: 20
                text: card.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                width: 200
                height: 18
                text: QbzSession.tr("Track", QbzSession.trRev) + " • " + (card.artist || "")
                color: theme.textMuted
                font.pixelSize: theme.fontLink - 1
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                visible: qualityLabel !== ""
                width: 200
                text: qualityLabel
                color: theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
        }
    }

    // Track row (primitives/TrackRow.slint, 50px — number/play, title +
    // explicit, artist, duration, quality, ⋯ menu).
    // "Load more (loaded / total)" (LoadMoreButton).
    component LoadMoreButton: Item {
        property int loaded: 0
        property int total: 0
        width: parent ? parent.width : 0
        visible: loaded < total
        height: visible ? 44 : 0
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 6
            width: lmText.implicitWidth + 36
            height: 32
            radius: theme.radiusSm
            color: lmArea.containsMouse ? theme.surfaceElevated : theme.surfaceCard
            border.width: 1
            border.color: theme.borderSubtle
            Text {
                id: lmText
                anchors.centerIn: parent
                text: QbzSession.tr("Load more", QbzSession.trRev) + " (" + loaded + " / " + total + ")"
                color: theme.textSecondary
                font.pixelSize: 13
            }
            MouseArea {
                id: lmArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzBridge.searchLoadMore(root.tab)
            }
        }
    }

    // searchType filter radio (FilterRadio: 14px ring + 8px accent dot).
    component FilterRadio: Item {
        property string label: ""
        property bool selected: false
        signal picked()
        width: frRow.width
        height: 22
        Row {
            id: frRow
            spacing: 6
            height: parent.height
            Rectangle {
                width: 14
                height: 14
                radius: 7
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                border.width: 1.5
                border.color: parent.parent.selected ? theme.accent : theme.textMuted
                Rectangle {
                    width: 8
                    height: 8
                    radius: 4
                    anchors.centerIn: parent
                    color: parent.parent.selected ? theme.accent : "transparent"
                }
            }
            Text {
                height: parent.height
                text: parent.parent.label
                color: (parent.parent.selected || frArea.containsMouse) ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
            }
        }
        MouseArea {
            id: frArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.picked()
        }
    }

    // ============================ the view ================================
    Column {
        anchors.fill: parent
        spacing: 0

        // --- Row 1: title (left) + filter radios (right) -------------------
        Item {
            width: parent.width
            height: 56
            Text {
                x: 32
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Search", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
            }
            Row {
                visible: root.hasResults
                anchors.right: parent.right
                anchors.rightMargin: 32
                anchors.verticalCenter: parent.verticalCenter
                spacing: 16
                FilterRadio { label: QbzSession.tr("Main Artist", QbzSession.trRev); selected: root.filterIndex === 1; onPicked: QbzBridge.searchFilterChanged(1) }
                FilterRadio { label: QbzSession.tr("Performer", QbzSession.trRev); selected: root.filterIndex === 2; onPicked: QbzBridge.searchFilterChanged(2) }
                FilterRadio { label: QbzSession.tr("Composer", QbzSession.trRev); selected: root.filterIndex === 3; onPicked: QbzBridge.searchFilterChanged(3) }
                FilterRadio { label: QbzSession.tr("Label", QbzSession.trRev); selected: root.filterIndex === 4; onPicked: QbzBridge.searchFilterChanged(4) }
                FilterRadio { label: QbzSession.tr("Release Name", QbzSession.trRev); selected: root.filterIndex === 5; onPicked: QbzBridge.searchFilterChanged(5) }
                Rectangle {
                    visible: root.filterIndex !== 0
                    width: 24
                    height: 24
                    radius: 12
                    anchors.verticalCenter: parent.verticalCenter
                    color: clrArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    QbzIcon { name: "x"; width: 13; height: 13; anchors.centerIn: parent; tintName: clrArea.containsMouse ? "primary" : "muted" }
                    MouseArea {
                        id: clrArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.searchFilterChanged(0)
                    }
                }
            }
        }

        // --- Row 2: the five-tab strip (collapsed during the initial load) --
        Item {
            visible: !(root.loading && !root.hasResults)
            width: parent.width
            height: 40
            Row {
                x: 32
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 8
                spacing: 22
                Repeater {
                    model: [
                        { "t": 0, "label": QbzSession.tr("All", QbzSession.trRev) },
                        { "t": 1, "label": QbzSession.tr("Albums", QbzSession.trRev) },
                        { "t": 2, "label": QbzSession.tr("Tracks", QbzSession.trRev) },
                        { "t": 3, "label": QbzSession.tr("Artists", QbzSession.trRev) },
                        { "t": 4, "label": QbzSession.tr("Playlists", QbzSession.trRev) },
                    ]
                    delegate: Column {
                        required property var modelData
                        spacing: 5
                        Text {
                            text: modelData.label
                            color: root.tab === modelData.t ? theme.textPrimary
                                : (stabArea.containsMouse ? theme.textSecondary : theme.textMuted)
                            font.pixelSize: 14
                            font.weight: root.tab === modelData.t ? theme.weightBold : theme.weightMedium
                            MouseArea {
                                id: stabArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: QbzBridge.searchTabChanged(modelData.t)
                            }
                        }
                        Rectangle {
                            width: parent.width
                            height: 2
                            radius: 1
                            color: root.tab === modelData.t ? theme.accent : "transparent"
                        }
                    }
                }
            }
        }

        // --- Scrollable body ------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 56 - 40
            Flickable {
                id: bodyFlick
                anchors.fill: parent
                contentWidth: width
                contentHeight: bodyCol.height + 32
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: bodyCol
                    x: 32
                    width: bodyFlick.width - 64
                    spacing: 28

                    // Fade in once the search completes (SearchResultsView).
                    opacity: root.loading ? 0.0 : 1.0
                    Behavior on opacity { NumberAnimation { duration: 280; easing.type: Easing.InOutQuad } }

                    // ---- Most popular + Artists (All tab) ------------------
                    Row {
                        visible: root.tab === 0 && (root.mp.kind || "") !== ""
                        width: parent.width
                        spacing: 24
                        // Hero column, fixed 200px (the carousel always starts
                        // at the same x regardless of the hero kind).
                        Column {
                            width: 200
                            spacing: 12
                            Row {
                                spacing: 8
                                QbzIcon { name: "award"; width: 18; height: 18; tintName: "warning" }
                                Text {
                                    text: QbzSession.tr("Most popular", QbzSession.trRev)
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontSection
                                    font.weight: theme.weightSemibold
                                }
                            }
                            Item {
                                width: 200
                                height: 246
                                ArtistCard {
                                    visible: root.mp.kind === "artist"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    item: root.mp.artist || ({})
                                    artSource: (root.mp.artist || ({})).artPath || ""
                                }
                                AlbumCard {
                                    visible: root.mp.kind === "album"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    albumId: (root.mp.album || ({})).id || ""
                                    title: (root.mp.album || ({})).title || ""
                                    artist: (root.mp.album || ({})).artist || ""
                                    artistId: (root.mp.album || ({})).artistId || ""
                                    genre: (root.mp.album || ({})).genre || ""
                                    year: (root.mp.album || ({})).year || ""
                                    qualityTier: (root.mp.album || ({})).qualityTier || ""
                                    artSource: (root.mp.album || ({})).artPath || ""
                                    isFavorite: false
                                    isPinned: false
                                }
                                SearchTrackHero {
                                    visible: root.mp.kind === "track"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    card: root.mp.track || ({})
                                    qualityLabel: root.mp.qualityLabel || ""
                                }
                            }
                        }
                        // Artists carousel — fills the rest of the row.
                        Column {
                            visible: root.artistsCarousel.length > 0
                            width: parent.width - 224
                            spacing: 12
                            QbzSectionHeader {
                                title: QbzSession.tr("Artists", QbzSession.trRev)
                                showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                                onViewAllClicked: QbzBridge.searchTabChanged(3)
                            }
                            ListView {
                                width: parent.width
                                height: 246
                                orientation: ListView.Horizontal
                                spacing: 32
                                clip: true
                                boundsBehavior: Flickable.StopAtBounds
                                model: root.artistsCarousel
                                delegate: ArtistCard {
                                    item: modelData
                                    artSource: modelData.artPath || ""
                                }
                            }
                        }
                    }
                    // Artists carousel when there is no most-popular hero.
                    Column {
                        visible: root.tab === 0 && (root.mp.kind || "") === "" && root.artists.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Artists", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(3)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.artists
                            delegate: ArtistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                        }
                    }

                    // ---- Albums --------------------------------------------
                    Column {
                        visible: root.tab === 0 && root.albums.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Albums", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(1)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.albums
                            delegate: AlbumCard {
                                albumId: modelData.id
                                title: modelData.title
                                artist: modelData.artist
                                artistId: modelData.artistId
                                genre: modelData.genre
                                year: modelData.year
                                qualityTier: modelData.qualityTier
                                artSource: modelData.artPath || ""
                                isFavorite: false
                                isPinned: false
                            }
                        }
                    }
                    // Albums tab: the full grid + Load more.
                    GridView {
                        visible: root.tab === 1
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 24) / 224))) * 270 - 24)
                        cellWidth: 224
                        cellHeight: 270
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.albums
                        delegate: AlbumCard {
                            albumId: modelData.id
                            title: modelData.title
                            artist: modelData.artist
                            artistId: modelData.artistId
                            genre: modelData.genre
                            year: modelData.year
                            qualityTier: modelData.qualityTier
                            artSource: modelData.artPath || ""
                            isFavorite: false
                            isPinned: false
                        }
                    }
                    LoadMoreButton {
                        visible: root.tab === 1
                        loaded: root.albums.length
                        total: root.doc.albumsTotal || 0
                    }

                    // ---- Tracks --------------------------------------------
                    Column {
                        visible: (root.tab === 0 || root.tab === 2) && root.tracks.length > 0
                        width: parent.width
                        spacing: 4
                        QbzSectionHeader {
                            title: QbzSession.tr("Tracks", QbzSession.trRev)
                            showViewAll: root.tab === 0 && (root.doc.tracksTotal || 0) > root.previewCap
                            viewAllAccent: true
                            showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(2)
                        }
                        Repeater {
                            model: root.tab === 2 ? root.tracks : root.tracks.slice(0, root.previewCap)
                            delegate: TrackRow {
                                item: modelData
                                number: index + 1
                                menuShowFavorite: false
                                onPlayRequested: QbzPlayer.playTrack(item.id)
                                onEnqueueRequested: function (m) { QbzPlayer.enqueueTrack(item.id, m) }
                            }
                        }
                    }
                    LoadMoreButton {
                        visible: root.tab === 2
                        loaded: root.tracks.length
                        total: root.doc.tracksTotal || 0
                    }

                    // ---- Artists grid (per-type tab) ------------------------
                    GridView {
                        visible: root.tab === 3
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 16) / 216))) * 262 - 16)
                        cellWidth: 216
                        cellHeight: 262
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.artists
                        delegate: ArtistCard {
                            item: modelData
                            artSource: modelData.artPath || ""
                        }
                    }
                    LoadMoreButton {
                        visible: root.tab === 3
                        loaded: root.artists.length
                        total: root.doc.artistsTotal || 0
                    }

                    // ---- Playlists ------------------------------------------
                    Column {
                        visible: root.tab === 0 && root.playlists.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Playlists", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(4)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.playlists
                            delegate: PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                        }
                    }
                    GridView {
                        visible: root.tab === 4
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 24) / 224))) * 270 - 24)
                        cellWidth: 224
                        cellHeight: 270
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.playlists
                        delegate: PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                    }
                    LoadMoreButton {
                        visible: root.tab === 4
                        loaded: root.playlists.length
                        total: root.doc.playlistsTotal || 0
                    }
                }

                // ---- Loading + empty states ---------------------------------
                Item {
                    visible: root.loading && !root.hasResults
                    anchors.fill: parent
                    Column {
                        anchors.centerIn: parent
                        spacing: 18
                        QbzSpinner { size: 36; anchors.horizontalCenter: parent.horizontalCenter }
                        Text {
                            text: QbzSession.tr("Searching…", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 13
                        }
                    }
                }
                Text {
                    visible: !root.loading && !root.hasResults
                    x: 32
                    y: 8
                    text: QbzSession.tr("No results.", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: 14
                }
            }
            QbzScrollBar {
                target: bodyFlick
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
            }
        }
    }
}
