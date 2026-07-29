// Discover > Home — QML port of crates/qbz-ui/ui/discover/HomeView.slint
// (+ Carousel / AlbumCard / SlimCarousel / SlimCard / ArtistCarousel /
// PlaylistCarousel / HomeSkeleton / OfflinePlaceholder).
//
// Data: QbzHome.homeSectionsJson (one JSON document — see bridge.rs),
// published by src/home_qt.rs; artwork file:// paths resolve through the
// qbz-cache image cache. Section kinds: "album" | "playlist" | "slim" |
// "slimTracks" | "artists" | "pinned" | "mixes" | "recentPlaceholder". Rail
// ORDER + VISIBILITY follow the persisted Discover prefs (phase 11,
// discover_prefs.db).
//
// POC-NOTEs:
// - The genre filter (shared GenreFilterPopup, context "discover") and the
//  section-configurator gear (DiscoverConfigModal) are WIRED: the genre
//  selection feeds get_discover_index and persists to genre_filter.json; the
//  configurator persists to discover_prefs.db and re-renders the tabs from
//  the cached section data. The gear is DISABLED on the Recommendations tab
//  — its Slint arm also carries the reco cache-window + "Refresh now"
//  controls, which DiscoverConfigModal.qml does not have, so it would open
//  empty.
// - Editor's Picks / For You mount the same rails, ordered by each tab's
//  discover prefs (phase 13). Recommendations is the external-reco engine
//  (src/recommendations_qt.rs), LAZY-loaded the first time the tab becomes
//  visible, and the tab itself is gated on the persisted showRecommendations
//  pref exactly as in Slint.
// - "View all" is LIVE. WHICH rails carry the link is decided HERE, per
//  TAB, by `viewAllKind()` — not in Rust: `home_qt::assemble()` clones ONE
//  candidate list into all three tab documents, and Slint's For You arm for
//  `recentlyPlayedAlbums` has NO link (ForYouView.slint:117-126) while
//  Home's does (HomeView.slint:411). Stamping the decision on the shared
//  candidate would leak the link onto For You. The four destinations:
//    endpoint != ""       -> DiscoverBrowse for that /discover/<module>
//    qobuzPlaylists       -> PlaylistBrowse   (Home + Editor's Picks only)
//    recentlyPlayedAlbums -> Recently Played  (Home only, non-placeholder)
//    mostPlayedAlbums     -> Most Played      (Home + For You)
// - Card clicks / hover actions (play / favorite / more / pin) are live;
//  context menus follow each card's own seam constants.
// - The offline mount mirrors AppShell's ADR-010 seam: OfflineState.offline
//  -> the OfflinePlaceholder replica (exact msgids).

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../theme"

Rectangle {
    id: root
    // Transparent while the ambient background is active (phase 14 —
    // HomeView.slint:163: the frosted content panel shows through).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
       
    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

    QbzTheme { id: theme }

    // Reparsed whenever Rust republishes the JSON documents (one per
    // Discover tab — phase 13).
    readonly property var sections: JSON.parse(QbzHome.homeSectionsJson)
    readonly property var editorSections: JSON.parse(QbzHome.editorSectionsJson)
    readonly property var forYouSections: JSON.parse(QbzHome.forYouSectionsJson)
    // Recommendations (the 4th tab). Lazy: the document stays "[]" until the
    // tab is first shown and src/recommendations_qt.rs publishes into it.
    readonly property var recoSections: JSON.parse(QbzHome.recoSectionsJson)
    property string activeTab: "home"

    // Slint gates the 4th tab on the persisted `show_recommendations` pref
    // (discover_prefs.db). The port publishes it inside the settings document
    // (settings_qt SettingsDoc.showRecommendations) — read it straight off the
    // bridge singleton, the NavFlyout precedent.
    readonly property bool showRecommendations: {
        try {
            return JSON.parse(QbzBridge.settingsJson).showRecommendations !== false
        } catch (e) {
            return true
        }
    }
    // Never leave the user parked on a tab that just disappeared.
    onShowRecommendationsChanged: {
        if (!showRecommendations && root.activeTab === "recommendations")
            root.activeTab = "home"
    }

    // --- shared genre filter, "discover" context -------------------------
    // The badge lives in controls/BrowseGenreButton.qml now (the browse
    // pages draw the same control). It reads QbzBridge.genreFilterJson
    // STRAIGHT off the bridge singleton, never off genrePopup: the popup is
    // declared LAST (z-order) and a creation-time binding that dereferences
    // a not-yet-created id registers NO dependency, so it would never
    // re-evaluate. The popup instance is only touched from click handlers,
    // which run long after creation.

    // --- skeleton pulse ---------------------------------------------------
    // ONE 900ms Timer drives EVERY placeholder in this view (QbzSkeleton's
    // preferred drive mode). GATING RULE: freeze on NOT VISIBLE — the view
    // hidden, or the window minimized/hidden. NEVER on lost focus (a tiling
    // desktop keeps windows visible and unfocused).
    property bool skelPhase: false
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    // Runs only while something can actually be shimmering: the first fetch
    // (section skeletons), a card still waiting for its cover, or the grace
    // window in which landed covers are still decoding (artHold).
    readonly property bool skelNeeded: QbzHome.homeLoading || QbzHome.recoLoading
        || root.artPending || root.artHold
    // The sections document the active tab is rendering.
    readonly property var activeSections:
        root.activeTab === "editorPicks" ? root.editorSections
        : root.activeTab === "forYou" ? root.forYouSections
        : root.activeTab === "recommendations" ? root.recoSections
        : root.sections
    // Cheap "some card in the mounted rails has artUrl but no artPath yet"
    // probe — recomputed only when a sections document is republished.
    readonly property bool artPending: root.anyArtPending(root.activeSections)
    function anyArtPending(model) {
        for (var s = 0; s < model.length; s++) {
            var items = model[s].items || []
            for (var i = 0; i < items.length; i++) {
                if ((items[i].artUrl || "") !== "" && (items[i].artPath || "") === "")
                    return true
            }
        }
        return false
    }
    // The pulse must OUTLIVE `artPending`. That flag drops when the last
    // PATH lands, but every one of those cards still has a decode and a
    // canvas raster ahead of it and its placeholder is still up (see
    // QbzSkeleton's handover) — without the grace the tiles would freeze
    // mid-shimmer for the rest of the wait.
    property bool artHold: false
    Timer { id: artHoldOff; interval: 1500; onTriggered: root.artHold = false }
    onArtPendingChanged: { root.artHold = true; artHoldOff.restart() }
    Timer {
        interval: 900
        repeat: true
        running: root.visible && root.windowShowing && root.skelNeeded
        onTriggered: root.skelPhase = !root.skelPhase
    }

    // ============================ "View all" ==============================

    /// Which full-list page a rail's "View all" opens on THIS tab, or "" for
    /// no link. Read the header note for why the decision lives here.
    ///   "discover"  -> DiscoverBrowse for `section.endpoint`
    ///   "playlists" -> PlaylistBrowse
    ///   "recent"    -> Recently Played Albums
    ///   "mostplayed"-> Most Played Albums
    function viewAllKind(s, tab) {
        if (!s)
            return ""
        // Generic album carousels + both mostStreamed variants: the section
        // carries the /discover endpoint the page pages through.
        if ((s.endpoint || "") !== "")
            return "discover"
        // HomeView.slint:355 mounts the playlist arm on the Home AND Editor's
        // Picks repeater; discover_prefs has no qobuzPlaylists entry for For
        // You, so this only ever fires on the two tabs that render it.
        if (s.id === "qobuzPlaylists" && (tab === "home" || tab === "editorPicks"))
            return "playlists"
        // Home only (ForYouView.slint's arm has no show-view-all), and never
        // on the empty-history placeholder (kind "recentPlaceholder").
        if (s.id === "recentlyPlayedAlbums" && tab === "home" && s.kind === "album")
            return "recent"
        // HomeView.slint:506 AND ForYouView.slint:152 — both tabs.
        if (s.id === "mostPlayedAlbums")
            return "mostplayed"
        return ""
    }

    /// The page title, when it differs from the rail title. Only the slim
    /// "Most Streamed" rail does: it is titled "Popular albums" on Home but
    /// HomeView.slint:466 hard-codes @tr("Most Streamed") for the page.
    function viewAllTitle(s) {
        if (s.id === "mostStreamed")
            return QbzSession.tr("Most Streamed", QbzSession.trRev)
        return s.title || ""
    }

    function openViewAll(s, tab) {
        var kind = root.viewAllKind(s, tab)
        if (kind === "discover")
            QbzHome.openDiscoverBrowse(s.endpoint || "", root.viewAllTitle(s))
        else if (kind === "playlists")
            QbzHome.openPlaylistBrowse()
        else if (kind === "recent")
            QbzHome.openRecentAlbums()
        else if (kind === "mostplayed")
            QbzHome.openMostPlayedAlbums()
    }

    // ============================ shared components =======================

    // Circular page-control button (Carousel's NavButton).
    // Horizontal album rail (Carousel.slint): header + clipped ListView,
    // page chevrons (per-page step like the Slint paging).
    component AlbumRail: Column {
        id: albumRail
        property var sectionData: ({})
        /// Which Discover tab is rendering this rail — the "View all"
        /// decision is per tab (see viewAllKind).
        property string tabId: "home"
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
        readonly property int step: perPage * 232
        readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)

        QbzSectionHeader {
            title: sectionData.title
            leftEnabled: rail.contentX > 1
            rightEnabled: rail.contentX < albumRail.maxScroll - 1
            showViewAll: root.viewAllKind(albumRail.sectionData, albumRail.tabId) !== ""
            onViewAllClicked: root.openViewAll(albumRail.sectionData, albumRail.tabId)
            // Math.MIN here paged LEFT to a permanent 0 (the chevron looked
            // enabled and did nothing once contentX passed the first step);
            // PinnedRail:408 and SlimGrid:341 both use max.
            onPageLeft: rail.contentX = Math.max(0, rail.contentX - albumRail.step)
            onPageRight: rail.contentX = Math.min(albumRail.maxScroll, rail.contentX + albumRail.step)
        }
        Item {
            width: parent.width
            height: 246
            ListView {
                id: rail
                anchors.fill: parent
                orientation: ListView.Horizontal
                spacing: 32
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: sectionData.items
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
                        ribbon: modelData.ribbon
                        ribbonKind: modelData.ribbonKind
                        artSource: modelData.artPath
                        isPinned: modelData.isPinned
                        // POC-NOTE: Home hearts are not seeded from fav_cache
                        // (store not open in the POC); toggles still hit the API.
                        isFavorite: false
                    }
                    // Per-item artwork placeholder: the grey tile shimmers
                    // until THIS card's cover is ON SCREEN, then dissolves
                    // into it — the rail fills in progressively instead of
                    // all at once. A bare Rectangle, so it does not eat the
                    // card's hover/click areas underneath.
                    // AlbumCard seals its RoundedImage away, so the handover
                    // uses the probe arm (`coverSource`): it rides the same
                    // pixmap-cache entry the card is loading and retires on
                    // the DECODE, never on the path merely appearing — the
                    // path lands while the card's canvas is still blank.
                    QbzSkeleton {
                        variant: "art"
                        width: 200
                        height: 200
                        pending: (modelData.artUrl || "") !== ""
                        coverSource: modelData.artPath || ""
                        phase: root.skelPhase
                        cellIndex: index
                        // A cover whose download fails republishes the
                        // document with an empty artPath and no further
                        // signal, so the tile must be bounded (same rule and
                        // constant as SearchView's CardArtSkeleton).
                        settleMs: 6000
                    }
                }
            }
            // Cider-style edge fades (Carousel.slint): content dissolves
            // into the page background at the scrolled edges.
            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 56
                opacity: rail.contentX > 1 ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: theme.surfaceMain }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
            Rectangle {
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: 56
                opacity: rail.contentX < maxScroll - 1 ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 1.0; color: theme.surfaceMain }
                }
            }
        }
    }

    // TrackSlimRow — the TRACK flavour of the slim row. Visually the shared
    // qml/cards/SlimCard.qml (same 60px row, 44px thumb, title/subtitle),
    // but the whole-row click PLAYS the track instead of opening an album.
    // SlimCard hardcodes `QbzAlbum.openAlbum(card.id)`, so mounting it for a
    // track row would navigate to an album id that does not exist — a control
    // that renders and does the WRONG thing. The consolidation (a `kind` /
    // `activated()` seam on SlimCard, then delete this) is in the handoff.
    component TrackSlimRow: Rectangle {
        property var card: ({})
        height: 60
        radius: theme.radiusSm
        color: trArea.containsMouse ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 12
            Rectangle {
                width: 44
                height: 44
                radius: 4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: card.artPath || ""
                    radius: 4
                }
            }
            Column {
                width: parent.width - 44 - 2 * 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: card.title || ""
                    color: theme.textPrimary
                    font.pixelSize: theme.fontLink
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: card.artist || ""
                    color: theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
        }
        MouseArea {
            id: trArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: QbzPlayer.playTrack(card.id)
        }
    }

    // Slim rows mount the shared qml/cards/SlimCard.qml (SlimCard.slint) for
    // ALBUM data and TrackSlimRow for TRACK data — one paging shell, two row
    // flavours (Slint's SlimCarousel is likewise shared by Popular albums and
    // Recently Played Tracks).
    // Popular slim grid (SlimCarousel.slint): 4x3 pages of 12, capped 24.
    component SlimGrid: Column {
        id: sgrid
        property var sectionData: ({})
        property string tabId: "home"
        // true = the rows are TRACKS (click plays), false = ALBUMS (click opens).
        property bool tracks: false
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: 12
        readonly property int total: Math.min(sectionData.items.length, 2 * perPage)
        readonly property int pageCount: Math.max(1, Math.ceil(total / perPage))
        readonly property real maxScroll: (pageCount - 1) * grid.width

        QbzSectionHeader {
            title: sectionData.title
            leftEnabled: grid.contentX > 1
            rightEnabled: grid.contentX < sgrid.maxScroll - 1
            showViewAll: root.viewAllKind(sgrid.sectionData, sgrid.tabId) !== ""
            onViewAllClicked: root.openViewAll(sgrid.sectionData, sgrid.tabId)
            onPageLeft: grid.contentX = Math.max(0, grid.contentX - grid.width)
            onPageRight: grid.contentX = Math.min(sgrid.maxScroll, grid.contentX + grid.width)
        }
        Item {
            width: parent.width
            height: 3 * 60 + 2 * 8
            clip: true
            Flickable {
                id: grid
                width: parent.width
                height: parent.height
                contentWidth: sgrid.pageCount * width
                contentHeight: height
                boundsBehavior: Flickable.StopAtBounds
                readonly property real cellWidth: (width - 3 * 8) / 4

                Repeater {
                    model: sectionData.items
                    delegate: Item {
                        required property var modelData
                        required property int index
                        readonly property int slot: index % sgrid.perPage
                        readonly property int pageIdx: Math.floor(index / sgrid.perPage)
                        visible: index < sgrid.total
                        width: grid.cellWidth
                        height: 60
                        x: pageIdx * grid.width + (slot % 4) * (grid.cellWidth + 8)
                        y: Math.floor(slot / 4) * (60 + 8)

                        // The PinnedRail dispatch pattern: Components declared
                        // in the delegate scope, so `modelData` resolves.
                        Component {
                            id: albumRowComp
                            SlimCard { card: modelData }
                        }
                        Component {
                            id: trackRowComp
                            TrackSlimRow { card: modelData }
                        }
                        Loader {
                            anchors.fill: parent
                            sourceComponent: sgrid.tracks ? trackRowComp : albumRowComp
                        }
                    }
                }
            }
        }
    }

    // Pinned rail (PinnedCarousel.slint) — one 200x246 slot per item, the
    // card picked by the item's own kind: albums reuse AlbumCard, artists
    // render the ArtistGridCard circle, playlists the PlaylistCard square.
    // Fed from the shared per-user pinned_items.db (home_qt "pinned"
    // section; most-recent first).
    component PinnedRail: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)
        readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
        readonly property int step: perPage * 232

        QbzSectionHeader {
            title: sectionData.title
            leftEnabled: rail.contentX > 1
            rightEnabled: rail.contentX < maxScroll - 1
            onPageLeft: rail.contentX = Math.max(0, rail.contentX - step)
            onPageRight: rail.contentX = Math.min(maxScroll, rail.contentX + step)
        }
        Item {
            width: parent.width
            height: 246
            ListView {
                id: rail
                anchors.fill: parent
                orientation: ListView.Horizontal
                spacing: 32
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: sectionData.items
                delegate: Item {
                    required property var modelData
                    required property int index
                    width: 200
                    height: 246

                    Component {
                        id: pAlbum
                        AlbumCard {
                            albumId: modelData.id
                            title: modelData.title
                            artist: modelData.artist
                            artSource: modelData.artPath || ""
                            isPinned: true
                            isFavorite: false
                        }
                    }
                    Component {
                        id: pArtist
                        ArtistCard {
                            item: modelData
                            artSource: modelData.artPath || ""
                            isPinned: true
                        }
                    }
                    Component {
                        id: pPlaylist
                        PlaylistCard {
                            // Pinned model: { id, title, artist, artPath,
                            // isPinned } — artist maps to the subtitle line.
                            item: Object.assign({}, modelData, { subtitle: modelData.subtitle || modelData.artist || "" })
                            artSource: modelData.artPath || ""
                            isPinned: true
                        }
                    }
                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData.itemKind === "artist" ? pArtist
                            : modelData.itemKind === "playlist" ? pPlaylist : pAlbum
                    }
                    // Square-art slots only — a pinned ARTIST keeps the
                    // designed round gradient+glyph placeholder ArtistCard
                    // already draws (Slint's ArtistGridCard), which reads as
                    // a portrait, not as a missing tile.
                    QbzSkeleton {
                        variant: "art"
                        width: 200
                        height: 200
                        pending: modelData.itemKind !== "artist"
                            && (modelData.artUrl || "") !== ""
                        coverSource: modelData.artPath || ""
                        phase: root.skelPhase
                        cellIndex: index
                        settleMs: 6000
                    }
                }
            }
        }
    }

    // Qobuz Mixes rail (QobuzMixesRow.slint) — four static 220px
    // navigation tiles (gradient art + badge + name, description below).
    // POC-NOTE: the mix DETAIL views are out of scope — tiles are inert.
    // (Slint's 135° linear gradients are approximated with corner
    // RadialGradients — QML has no angled linear gradient.)
    component MixTile: Column {
        property string badge: ""
        property string mixName: ""
        property string desc: ""
        property color c0: "#000000"
        property color c1: "#000000"
        property color c2: "#000000"
        spacing: 8
        width: 220

        Rectangle {
            width: 220
            height: 220
            radius: 8
            gradient: Gradient {
                GradientStop { position: 0.0; color: c0 }
                GradientStop { position: 0.5; color: c1 }
                GradientStop { position: 1.0; color: c2 }
            }
            // Fake the 135° sweep with a corner-centered radial overlay.
            Rectangle {
                anchors.fill: parent
                radius: 8
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "#00000000" }
                    GradientStop { position: 1.0; color: "#33000000" }
                }
            }
            Text {
                x: 12
                y: 12
                text: badge
                color: "#ccffffff"
                font.pixelSize: 10
                font.weight: theme.weightSemibold
                font.letterSpacing: 1
            }
            Text {
                anchors.centerIn: parent
                text: mixName
                color: "#ffffff"
                font.pixelSize: 22
                font.weight: theme.weightBold
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                // Inert (mix detail views out of scope).
            }
        }
        Text {
            width: 220
            text: desc
            color: theme.textMuted
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }
    }
    component MixesRail: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12
        Text {
            text: sectionData.title
            color: theme.textPrimary
            font.pixelSize: theme.fontSection
            font.weight: theme.weightSemibold
        }
        Row {
            spacing: 32
            MixTile {
                badge: "qobuz"; mixName: "DailyQ"
                desc: QbzSession.tr("Elevate your day with a customized selection of music.", QbzSession.trRev)
                c0: "#1e3a8a"; c1: "#6366f1"; c2: "#c084fc"
            }
            MixTile {
                badge: "qobuz"; mixName: "WeeklyQ"
                desc: QbzSession.tr("Take a weekly journey with a fresh mix every Friday.", QbzSession.trRev)
                c0: "#065f46"; c1: "#10b981"; c2: "#fbbf24"
            }
            MixTile {
                badge: "qbz"; mixName: "FavQ"
                desc: QbzSession.tr("A fresh shuffle from your personal library.", QbzSession.trRev)
                c0: "#7f1d1d"; c1: "#ef4444"; c2: "#fb923c"
            }
            MixTile {
                badge: "qbz"; mixName: "TopQ"
                desc: QbzSession.tr("Discover new music from your most-played playlists.", QbzSession.trRev)
                c0: "#1f2937"; c1: "#4b5563"; c2: "#fbbf24"
            }
        }
    }

    // The section-rails renderer (one per Discover tab — the tab bodies
    // differ only in WHICH sections doc they mount).
    component SectionRails: Column {
        id: rails
        property var sectionsModel: []
        /// "home" | "editorPicks" | "forYou" | "recommendations" — forwarded
        /// to every rail so its "View all" resolves per tab.
        property string tabId: "home"
        width: parent ? parent.width : 0
        spacing: 40

        Repeater {
            model: sectionsModel
            delegate: Loader {
                required property var modelData
                property string railTab: rails.tabId
                width: parent ? parent.width : 0
                sourceComponent: modelData.kind === "album" ? albumRailComp
                    : modelData.kind === "playlist" ? playlistRailComp
                    : modelData.kind === "slim" ? slimGridComp
                    : modelData.kind === "slimTracks" ? trackGridComp
                    : modelData.kind === "artists" ? artistRailComp
                    : modelData.kind === "pinned" ? pinnedRailComp
                    : modelData.kind === "mixes" ? mixesRailComp
                    : recentComp
                property var sectionData: modelData

                Component {
                    id: pinnedRailComp
                    PinnedRail { sectionData: parent.sectionData }
                }
                Component {
                    id: albumRailComp
                    AlbumRail { sectionData: parent.sectionData; tabId: parent.railTab }
                }
                Component {
                    id: playlistRailComp
                    Column {
                        id: plRail
                        property var sectionData: parent.sectionData
                        property string tabId: parent.railTab
                        width: parent ? parent.width : 0
                        spacing: 12

                        readonly property int perPage: Math.max(1, Math.floor((plList.width + 32) / 232))
                        readonly property int step: perPage * 232
                        readonly property real maxScroll: Math.max(0, plList.contentWidth - plList.width)

                        // The rail used to draw a bare Text: no chevrons (so
                        // a 40-playlist rail's tail was unreachable) and no
                        // "View all". PlaylistCarousel.slint:109-115 has both.
                        // POC-NOTE: the Slint arm ALSO puts a PlaylistTagFilter
                        // on this title line (HomeView.slint:360-374) and shows
                        // @tr("No playlists match the selected categories.")
                        // when the tags filter everything out. Neither is
                        // ported here — the tag filter DOES exist on the
                        // PlaylistBrowse page this link opens.
                        QbzSectionHeader {
                            title: plRail.sectionData.title
                            leftEnabled: plList.contentX > 1
                            rightEnabled: plList.contentX < plRail.maxScroll - 1
                            showViewAll: root.viewAllKind(plRail.sectionData, plRail.tabId) !== ""
                            onViewAllClicked: root.openViewAll(plRail.sectionData, plRail.tabId)
                            onPageLeft: plList.contentX = Math.max(0, plList.contentX - plRail.step)
                            onPageRight: plList.contentX = Math.min(plRail.maxScroll, plList.contentX + plRail.step)
                        }
                        ListView {
                            id: plList
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: plRail.sectionData.items
                            delegate: PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                        }
                    }
                }
                Component {
                    id: slimGridComp
                    SlimGrid { sectionData: parent.sectionData; tabId: parent.railTab }
                }
                Component {
                    id: trackGridComp
                    SlimGrid { sectionData: parent.sectionData; tabId: parent.railTab; tracks: true }
                }
                Component {
                    id: artistRailComp
                    Column {
                        id: arRail
                        property var sectionData: parent.sectionData
                        width: parent ? parent.width : 0
                        spacing: 12

                        readonly property int perPage: Math.max(1, Math.floor((arList.width + 32) / 232))
                        readonly property int step: perPage * 232
                        readonly property real maxScroll: Math.max(0, arList.contentWidth - arList.width)

                        // ArtistCarousel.slint:141-147 pages with chevrons; a
                        // bare Text left the tail of an 18-artist rail
                        // unreachable. No "View all" — the Slint artist arms
                        // (topArtists / artistsToFollow) carry none.
                        QbzSectionHeader {
                            title: arRail.sectionData.title
                            leftEnabled: arList.contentX > 1
                            rightEnabled: arList.contentX < arRail.maxScroll - 1
                            onPageLeft: arList.contentX = Math.max(0, arList.contentX - arRail.step)
                            onPageRight: arList.contentX = Math.min(arRail.maxScroll, arList.contentX + arRail.step)
                        }
                        ListView {
                            id: arList
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: arRail.sectionData.items
                            delegate: ArtistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                        }
                    }
                }
                Component {
                    id: mixesRailComp
                    MixesRail { sectionData: parent.sectionData }
                }
                Component {
                    id: recentComp
                    QbzEmptyState {
                        property var sectionData: parent.sectionData
                        title: sectionData.title
                        body: sectionData.hint
                    }
                }
            }
        }
    }

    // HomeSkeleton.slint's SkeletonRow, 1:1 on the shared QbzSkeleton
    // control: a 180x22 title bar over five 200px card placeholders,
    // spacing 32/12/8. `phase` comes from the ONE Timer on the view root.
    component SkeletonRow: Column {
        id: skelRow
        property bool phase: false
        width: parent ? parent.width : 0
        spacing: 12
        QbzSkeleton { variant: "block"; width: 180; height: 22; phase: skelRow.phase }
        Row {
            spacing: 32
            Repeater {
                model: 5
                delegate: QbzSkeleton {
                    required property int index
                    variant: "card"
                    width: 200
                    phase: skelRow.phase
                    cellIndex: index
                }
            }
        }
    }
    // Two SkeletonRows = the whole HomeSkeleton component. Mounted by each
    // Discover tab while its sections document is still empty.
    component TabSkeleton: Column {
        id: tabSkel
        property bool phase: false
        width: parent ? parent.width : 0
        spacing: 40
        SkeletonRow { phase: tabSkel.phase }
        SkeletonRow { phase: tabSkel.phase }
    }

    // ============================ offline gate ============================
    // (OfflinePlaceholder.slint replica; mounted INSTEAD of the view.)
    QbzOfflinePlaceholder {
        visible: QbzSession.offline
        anchors.centerIn: parent
        // The induced-only "Open Settings" arm (Slint) is wired:
        showSettingsAction: true
        onSettingsClicked: QbzShell.navigateTo("settings")
    }

    // ============================ the view ================================
    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzSession.offline

        // --- Toolbar (fixed 56px) ---------------------------------------
        Item {
            width: parent.width
            height: 56

            Row {
                // Slint left-controls: x 32 + NavButtons (now a 0px
                // placeholder) + 16px spacing -> the pill starts at 48.
                x: 48
                y: 25 - height / 2
                spacing: 16

                QbzTabBar {
                    // The 4th tab is present only while the pref is on (1:1
                    // with the Slint `if SettingsState.show-recommendations`).
                    tabs: {
                        var t = [
                            { "id": "home", "label": QbzSession.tr("Home", QbzSession.trRev) },
                            { "id": "editorPicks", "label": QbzSession.tr("Editor's Picks", QbzSession.trRev) },
                            { "id": "forYou", "label": QbzSession.tr("For You", QbzSession.trRev) },
                        ]
                        if (root.showRecommendations)
                            t.push({ "id": "recommendations", "label": QbzSession.tr("Recommendations", QbzSession.trRev) })
                        return t
                    }
                    activeId: root.activeTab
                    // Data is per-tab JSON (no refetch on switch); scroll
                    // resets to top.
                    onSelected: function (id) {
                        root.activeTab = id
                        homeFlick.contentY = 0
                    }
                }
            }

            // Genre filter + configurator gear (HomeView.slint right-controls).
            Row {
                x: parent.width - width - 32
                y: 25 - height / 2
                height: 32
                spacing: 6

                // GenreButton — now the SHARED controls/BrowseGenreButton
                // (the browse pages draw the same control; this was a
                // verbatim copy of it). HomeView.slint:85 is the 32px
                // variant, BrowseHeaderTools.slint:108 the 34px one.
                BrowseGenreButton {
                    context: "discover"
                    btnHeight: 32
                    onClicked: genrePopup.toggle()
                }

                // GearButton — per-tab show/hide + reorder of the Discover
                // sections. Disabled on Recommendations: its Slint arm
                // configures the external reco engine (not ported), so the
                // modal would open empty.
                Rectangle {
                    id: gearBtn
                    readonly property bool gearEnabled: root.activeTab !== "recommendations"
                    width: 32
                    height: 32
                    radius: 4
                    opacity: gearBtn.gearEnabled ? 1.0 : 0.35
                    color: (gearBtn.gearEnabled && gearArea.containsMouse)
                        ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: "home-gear"
                        width: 20
                        height: 20
                        anchors.centerIn: parent
                        tintName: gearArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: gearArea
                        anchors.fill: parent
                        enabled: gearBtn.gearEnabled
                        hoverEnabled: gearBtn.gearEnabled
                        cursorShape: gearBtn.gearEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: configModal.open(root.activeTab)
                    }
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // --- Scrollable sections -----------------------------------------
        Item {
            width: parent.width
            height: parent.height - 57
        Flickable {
            id: homeFlick
            width: parent.width
            height: parent.height
            clip: true
            contentWidth: width
            contentHeight: homeContent.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: homeContent
                width: parent.width
                padding: 32
                spacing: 40

                // ===== Home tab ==========================================
                Column {
                    id: homeTab
                    visible: root.activeTab === "home"
                    width: parent.width - 64
                    spacing: 40

                    // Loading skeleton (HomeSkeleton: two shimmer rows). The
                    // pulse comes from the view-root Timer, which is itself
                    // gated on visibility + window state.
                    TabSkeleton {
                        visible: QbzHome.homeLoading && root.sections.length === 0
                        phase: root.skelPhase
                    }

                    // Error state with retry (the Slint Home has no error
                    // arm; the box mirrors the FavoritesView Retry button).
                    Rectangle {
                        visible: QbzHome.homeError !== ""
                        width: parent.width
                        height: errorColumn.height + 28
                        radius: theme.radiusSm
                        color: theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle
                        Column {
                            id: errorColumn
                            anchors.centerIn: parent
                            spacing: 10
                            Text {
                                text: QbzHome.homeError
                                color: theme.textSecondary
                                font.pixelSize: 13
                            }
                            Rectangle {
                                width: retryText.implicitWidth + 28
                                height: 32
                                radius: 6
                                color: retryArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                                border.width: 1
                                border.color: theme.borderSubtle
                                Text {
                                    id: retryText
                                    anchors.centerIn: parent
                                    text: QbzSession.tr("Retry", QbzSession.trRev)
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontLegal
                                }
                                MouseArea {
                                    id: retryArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: QbzHome.reloadHome()
                                }
                            }
                        }
                    }

                    // Section rails.
                    SectionRails { sectionsModel: root.sections; tabId: "home" }
                }

                // ===== Editor's Picks (phase 13) ========================
                Column {
                    visible: root.activeTab === "editorPicks"
                    width: parent.width - 64
                    spacing: 40
                    TabSkeleton {
                        visible: QbzHome.homeLoading && root.editorSections.length === 0
                        phase: root.skelPhase
                    }
                    SectionRails { sectionsModel: root.editorSections; tabId: "editorPicks" }
                }

                // ===== For You (phase 13) =================================
                Column {
                    visible: root.activeTab === "forYou"
                    width: parent.width - 64
                    spacing: 40
                    TabSkeleton {
                        visible: QbzHome.homeLoading && root.forYouSections.length === 0
                        phase: root.skelPhase
                    }
                    SectionRails { sectionsModel: root.forYouSections; tabId: "forYou" }
                }

                // ===== Recommendations (external reco engine) =============
                // qbz-external-reco (Last.fm + ListenBrainz -> Qobuz), driven
                // by src/recommendations_qt.rs. LAZY: the first time this
                // column becomes visible it asks Rust to build the tab; every
                // later entry repaints from memory / the engine's own result
                // cache, so reopening Discover costs no external traffic.
                //
                // The load is kicked BOTH on mount and on becoming visible:
                // the view is instantiated while Home is the active tab, so a
                // visible-only hook would be thrown away on the mount pass and
                // a mount-only hook would never fire for a tab switch.
                Column {
                    id: recoTab
                    visible: root.activeTab === "recommendations"
                    width: parent.width - 64
                    spacing: 40

                    function kick() {
                        if (recoTab.visible)
                            QbzHome.loadRecommendations()
                    }
                    Component.onCompleted: recoTab.kick()
                    onVisibleChanged: recoTab.kick()

                    // Same two-row shimmer as the other tabs, while the first
                    // build is in flight and nothing has painted yet.
                    TabSkeleton {
                        visible: QbzHome.recoLoading && root.recoSections.length === 0
                        phase: root.skelPhase
                    }

                    // Rails resolve progressively — a row that is still
                    // building, or whose service is not connected, is simply
                    // ABSENT from the document (never an empty frame).
                    SectionRails { sectionsModel: root.recoSections; tabId: "recommendations" }

                    // Nothing built and nothing in flight: the Slint empty
                    // state, verbatim msgids.
                    QbzEmptyState {
                        visible: !QbzHome.recoLoading && root.recoSections.length === 0
                        width: parent.width
                        title: QbzSession.tr("No recommendations yet", QbzSession.trRev)
                        body: QbzSession.tr("Connect Last.fm or ListenBrainz in Settings, or play more music, to get personalized recommendations.", QbzSession.trRev)
                        actionLabel: QbzSession.tr("Open Settings", QbzSession.trRev)
                        onActionClicked: QbzShell.navigateTo("settings")
                    }
                }
            }
        }
        // Thin auto-hiding scrollbar in the right gutter (ListScrollbar).
        QbzScrollBar {
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            target: homeFlick
        }
        }
    }

    // ============================ overlays ================================
    // Declared LAST: in QML declaration order IS z-order, so these sit above
    // the toolbar and the rails and keep their own presses (ADR-009 is also
    // satisfied explicitly by their z). Both are hidden + disabled while
    // closed, so they never eat a click meant for the view.

    // Shared Filter-by-genre popup, "discover" context: the selection feeds
    // get_discover_index, so toggling one re-fetches the index.
    GenreFilterPopup {
        id: genrePopup
        anchors.fill: parent
        context: "discover"
        // Under the toolbar (56px + the 1px divider + a 5px gap).
        anchorTop: 62
        anchorRight: 32
    }

    // Per-tab section configurator (the gear).
    DiscoverConfigModal {
        id: configModal
        anchors.fill: parent
    }
}
