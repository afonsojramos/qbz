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
//
// LOADING (a deliberate ADDITION over the Slint's bare LoadingSpinner): the
// view mounts QbzSkeleton placeholders in the shape of the tab that is
// coming, plus a per-card cover placeholder that clears when THAT card's
// cover lands. See the "skeleton plumbing" block below for the cost.
//
// SIZE: this file is over the 500-line guideline (it already was at 638
// before the skeleton work). It is NOT split because every section is one
// tab of one document and the split would be arbitrary; the skeleton
// additions are the composite mounts, which are ~8 lines each precisely to
// keep it from growing further.

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../rows"
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

    // ======================= skeleton plumbing ===========================
    // ONE 900ms Timer drives EVERY placeholder in this view (QbzSkeleton's
    // preferred drive mode). GATING RULE: freeze on NOT VISIBLE — the view
    // hidden, or the window minimized/hidden. NEVER on lost focus (a tiling
    // desktop keeps windows visible and unfocused).
    property bool skelPhase: false
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true

    // A card is pending while it HAS a cover url and no local path yet.
    // Recomputed only when the document is republished (which is also when
    // covers land), never per frame.
    function anyPending(rows) {
        for (var i = 0; i < rows.length; i++) {
            if ((rows[i].artUrl || "") !== "" && (rows[i].artPath || "") === "") return true
        }
        return false
    }
    readonly property bool heroPending: {
        var k = mp.kind || ""
        if (k === "" || k === "artist") return false
        var c = k === "album" ? (mp.album || ({})) : (mp.track || ({}))
        return (c.artUrl || "") !== "" && (c.artPath || "") === ""
    }
    readonly property bool artPending: anyPending(albums) || anyPending(tracks)
        || anyPending(playlists) || heroPending
    // The pulse must OUTLIVE `artPending`: that flag drops when the last PATH
    // lands, but each of those cards still has a decode + canvas raster ahead
    // of it and its placeholder is still up (QbzSkeleton's handover). Without
    // the grace the tiles freeze mid-shimmer for the rest of the wait.
    property bool artHold: false
    Timer { id: artHoldOff; interval: 1500; onTriggered: root.artHold = false }
    onArtPendingChanged: { root.artHold = true; artHoldOff.restart() }

    // "Load more" has NO bridge-side busy flag: search_qt.rs only sets
    // doc.loading in submit(), never in load_more(). So the appended-page
    // placeholder is tracked locally — armed on the click, disarmed the
    // moment that tab's array actually grows. A page that comes back empty
    // (end of the result set) never grows it, which is why every appended
    // placeholder carries settleMs.
    property int moreTab: -1
    property int moreFrom: 0
    function rowsFor(t) {
        return t === 1 ? albums : t === 2 ? tracks
            : t === 3 ? artists : t === 4 ? playlists : []
    }
    readonly property bool morePending: root.moreTab >= 0
        && root.moreTab === root.tab
        && root.rowsFor(root.moreTab).length === root.moreFrom
    function armLoadMore(t) {
        root.moreTab = t
        root.moreFrom = root.rowsFor(t).length
        QbzBridge.searchLoadMore(t)
    }

    Timer {
        interval: 900
        repeat: true
        running: (root.loading || root.artPending || root.artHold || root.morePending)
            && root.visible && root.windowShowing
        onTriggered: root.skelPhase = !root.skelPhase
    }

    // Per-item cover placeholder, mounted by every card delegate over the
    // 200x200 artwork square. A bare Rectangle, so it does not eat the
    // card's hover/click areas underneath; it clears when THIS card's cover
    // lands, so a page resolves progressively instead of as a lump.
    // ARTISTS are deliberately excluded everywhere: ArtistCard already draws
    // a designed round gradient+glyph portrait for a missing cover, which
    // reads as a portrait rather than as a hole (same rule as LibraryView).
    // NOTE: an inline `component` does NOT see this file's outer ids, so it
    // takes NO `root.` reference — every call site passes `phase:` itself.
    component CardArtSkeleton: QbzSkeleton {
        property var card: ({})
        variant: "art"
        width: 200
        height: 200
        // HANDOVER, not "the path landed": the cards seal their RoundedImage
        // away, so this rides QbzSkeleton's probe arm — the same pixmap-cache
        // entry the card is loading, so no second decode — and retires when
        // that decode finishes. Gating on `artPath !== ""` (what this used to
        // do) drops the placeholder while the card's canvas is still blank.
        pending: (card.artUrl || "") !== ""
        coverSource: card.artPath || ""
        // A cover whose download fails republishes the document with an
        // empty artPath — without this the tile would shimmer forever.
        settleMs: 6000
    }

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
        // The click is reported OUT (the call site arms the appended-page
        // placeholder and then calls the bridge). An inline component does
        // not see this file's outer ids, so it must not reach for `root`.
        signal loadMore()
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
                onClicked: parent.parent.loadMore()
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
                    QbzIcon { name: "x"; width: 13; height: 13; anchors.centerIn: parent; tintName: clrArea.containsMouse ? "textPrimary" : "muted" }
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
                                    // Same two fields the album hero below
                                    // carries: the pin state (or the glyph
                                    // lies and the first click UN-pins) and
                                    // the REMOTE url the pin payload stores
                                    // (artPath is a local cache path).
                                    isPinned: (root.mp.artist || ({})).isPinned === true
                                    artworkUrl: (root.mp.artist || ({})).artUrl || ""
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
                                    // search_qt::map_album stamps the heart
                                    // from fav_cache; false inverted the
                                    // first click on an album already in the
                                    // library.
                                    isFavorite: (root.mp.album || ({})).isFavorite === true
                                    // The row carries the pin state
                                    // (search_qt `map_album`); hardcoding
                                    // false made the glyph lie and turned
                                    // the first click into an UN-pin.
                                    isPinned: (root.mp.album || ({})).isPinned === true
                                    artworkUrl: (root.mp.album || ({})).artUrl || ""
                                }
                                SearchTrackHero {
                                    visible: root.mp.kind === "track"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    card: root.mp.track || ({})
                                    qualityLabel: root.mp.qualityLabel || ""
                                }
                                // Hero cover placeholder (album/track arms —
                                // the artist arm keeps ArtistCard's designed
                                // round portrait).
                                CardArtSkeleton {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    card: root.mp.kind === "album" ? (root.mp.album || ({}))
                                        : root.mp.kind === "track" ? (root.mp.track || ({}))
                                        : ({})
                                    phase: root.skelPhase
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
                                    isPinned: modelData.isPinned === true
                                    artworkUrl: modelData.artUrl || ""
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
                                isPinned: modelData.isPinned === true
                                artworkUrl: modelData.artUrl || ""
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
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: 200
                                height: 246
                                AlbumCard {
                                    albumId: modelData.id
                                    title: modelData.title
                                    artist: modelData.artist
                                    artistId: modelData.artistId
                                    genre: modelData.genre
                                    year: modelData.year
                                    qualityTier: modelData.qualityTier
                                    artSource: modelData.artPath || ""
                                    // search_qt::map_album stamps the heart
                                    // from fav_cache; false made the glyph lie
                                    // and inverted the first click.
                                    isFavorite: modelData.isFavorite === true
                                    isPinned: modelData.isPinned === true
                                    artworkUrl: modelData.artUrl || ""
                                }
                                CardArtSkeleton {
                                    card: modelData
                                    phase: root.skelPhase
                                    cellIndex: index
                                }
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
                        delegate: Item {
                            required property var modelData
                            required property int index
                            width: 200
                            height: 246
                            AlbumCard {
                                albumId: modelData.id
                                title: modelData.title
                                artist: modelData.artist
                                artistId: modelData.artistId
                                genre: modelData.genre
                                year: modelData.year
                                qualityTier: modelData.qualityTier
                                artSource: modelData.artPath || ""
                                isFavorite: modelData.isFavorite === true
                                isPinned: modelData.isPinned === true
                                artworkUrl: modelData.artUrl || ""
                            }
                            CardArtSkeleton {
                                card: modelData
                                phase: root.skelPhase
                                cellIndex: index
                            }
                        }
                    }
                    // The page that "Load more" asked for, in the shape it
                    // will arrive in. One row of cells, one animator; it
                    // disappears the moment the array grows (or settles out
                    // if the page comes back empty).
                    QbzSkeleton {
                        visible: root.tab === 1 && root.morePending
                        variant: "cardGrid"
                        width: parent.width
                        height: visible ? 270 : 0
                        cellW: 224
                        cellH: 270
                        phase: root.skelPhase
                        settleMs: 8000
                    }
                    LoadMoreButton {
                        visible: root.tab === 1
                        loaded: root.albums.length
                        total: root.doc.albumsTotal || 0
                        onLoadMore: root.armLoadMore(1)
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
                    QbzSkeleton {
                        visible: root.tab === 2 && root.morePending
                        variant: "rowList"
                        width: parent.width
                        height: visible ? 200 : 0
                        rowH: 50
                        rowGap: 0
                        rowArtSize: 36
                        phase: root.skelPhase
                        settleMs: 8000
                    }
                    LoadMoreButton {
                        visible: root.tab === 2
                        loaded: root.tracks.length
                        total: root.doc.tracksTotal || 0
                        onLoadMore: root.armLoadMore(2)
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
                            isPinned: modelData.isPinned === true
                            artworkUrl: modelData.artUrl || ""
                        }
                    }
                    // Artists: ROUND cells (ArtistGridCard's portrait), and
                    // no per-item overlay anywhere — a missing artist photo
                    // has a designed placeholder already.
                    QbzSkeleton {
                        visible: root.tab === 3 && root.morePending
                        variant: "cardGrid"
                        width: parent.width
                        height: visible ? 262 : 0
                        cellW: 216
                        cellH: 262
                        roundCells: true
                        phase: root.skelPhase
                        settleMs: 8000
                    }
                    LoadMoreButton {
                        visible: root.tab === 3
                        loaded: root.artists.length
                        total: root.doc.artistsTotal || 0
                        onLoadMore: root.armLoadMore(3)
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
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: 200
                                height: 246
                                PlaylistCard {
                                    // artworkUrl is not passed: the card
                                    // defaults it to `item.artUrl`, which is
                                    // this row's remote cover.
                                    item: modelData
                                    artSource: modelData.artPath || ""
                                    isPinned: modelData.isPinned === true
                                }
                                CardArtSkeleton {
                                    card: modelData
                                    phase: root.skelPhase
                                    cellIndex: index
                                }
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
                        delegate: Item {
                            required property var modelData
                            required property int index
                            width: 200
                            height: 246
                            PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                                isPinned: modelData.isPinned === true
                            }
                            CardArtSkeleton {
                                card: modelData
                                phase: root.skelPhase
                                cellIndex: index
                            }
                        }
                    }
                    QbzSkeleton {
                        visible: root.tab === 4 && root.morePending
                        variant: "cardGrid"
                        width: parent.width
                        height: visible ? 270 : 0
                        cellW: 224
                        cellH: 270
                        phase: root.skelPhase
                        settleMs: 8000
                    }
                    LoadMoreButton {
                        visible: root.tab === 4
                        loaded: root.playlists.length
                        total: root.doc.playlistsTotal || 0
                        onLoadMore: root.armLoadMore(4)
                    }
                }

                // ---- Loading skeleton ---------------------------------------
                // Shape of the tab that is coming, not a spinner. Mounted for
                // the WHOLE of `loading`, not only the first search: bodyCol
                // fades to 0 while a new query is in flight, so a re-search
                // used to show an empty page with no affordance at all.
                // COST: at most 8 animators on the All tab (3 title bars, 1
                // hero card, 2 card composites, 1 slim-row composite); one
                // composite covers a whole viewport row on ONE animator.
                Item {
                    id: searchSkel
                    visible: root.loading
                    x: 32
                    y: 0
                    width: bodyFlick.width - 64
                    height: bodyFlick.height

                    // All tab: hero + artists carousel + albums + tracks.
                    Column {
                        visible: root.tab === 0
                        width: parent.width
                        spacing: 28
                        Row {
                            width: parent.width
                            spacing: 24
                            Column {
                                width: 200
                                spacing: 12
                                QbzSkeleton { variant: "block"; width: 160; height: 22; phase: root.skelPhase }
                                QbzSkeleton { variant: "card"; width: 200; phase: root.skelPhase }
                            }
                            Column {
                                width: parent.width - 224
                                spacing: 12
                                QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                                QbzSkeleton {
                                    variant: "cardGrid"
                                    width: parent.width
                                    height: 246
                                    cellW: 232
                                    cellH: 246
                                    roundCells: true
                                    phase: root.skelPhase
                                }
                            }
                        }
                        Column {
                            width: parent.width
                            spacing: 12
                            QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                            QbzSkeleton {
                                variant: "cardGrid"
                                width: parent.width
                                height: 246
                                cellW: 232
                                cellH: 246
                                phase: root.skelPhase
                            }
                        }
                        Column {
                            width: parent.width
                            spacing: 8
                            QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                            QbzSkeleton {
                                variant: "rowList"
                                width: parent.width
                                height: 300
                                rowH: 50
                                rowGap: 0
                                rowArtSize: 36
                                phase: root.skelPhase
                            }
                        }
                    }

                    // Albums / Playlists tabs: the 224x270 grid.
                    QbzSkeleton {
                        visible: root.tab === 1 || root.tab === 4
                        variant: "cardGrid"
                        anchors.fill: parent
                        cellW: 224
                        cellH: 270
                        phase: root.skelPhase
                    }
                    // Tracks tab: the 50px TrackRow list.
                    QbzSkeleton {
                        visible: root.tab === 2
                        variant: "rowList"
                        anchors.fill: parent
                        rowH: 50
                        rowGap: 0
                        rowArtSize: 36
                        phase: root.skelPhase
                    }
                    // Artists tab: round cells on the 216x262 pitch.
                    QbzSkeleton {
                        visible: root.tab === 3
                        variant: "cardGrid"
                        anchors.fill: parent
                        cellW: 216
                        cellH: 262
                        roundCells: true
                        phase: root.skelPhase
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
