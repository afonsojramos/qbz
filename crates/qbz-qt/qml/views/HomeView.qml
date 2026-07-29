// Discover > Home — QML port of crates/qbz-ui/ui/discover/HomeView.slint
// (+ Carousel / AlbumCard / SlimCarousel / SlimCard / ArtistCarousel /
// PlaylistCarousel / HomeSkeleton / OfflinePlaceholder).
//
// Data: QbzHome.homeSectionsJson (one JSON document — see bridge.rs),
// published by src/home_qt.rs; artwork file:// paths resolve through the
// qbz-cache image cache. Section kinds: "album" | "playlist" | "slim" |
// "artists" | "pinned" | "recentPlaceholder". Rail ORDER + VISIBILITY
// follow the persisted Discover prefs (phase 11, discover_prefs.db).
//
// POC-NOTEs:
// - The genre filter + section-configurator gear are INERT visual stubs
//  (out of scope).
// - Editor's Picks / For You mount the same rails, ordered by each tab's
//  discover prefs (phase 13); Recommendations renders a placeholder — the
//  external reco engine (external_reco.rs) is not ported. The tab bar is
//  always fully visible (the Slint showRecommendations gate is not wired;
//  the pref is ON for this user anyway).
// - Card clicks / hover actions (play / favorite / more / pin) and
//  "View all" / context menus are inert — album/artist pages, playback
//  and per-user stores are later phases.
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
    property string activeTab: "home"

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
    readonly property bool skelNeeded: QbzHome.homeLoading || root.artPending
        || root.artHold
    // Cheap "some card in the mounted rails has artUrl but no artPath yet"
    // probe — recomputed only when a sections document is republished.
    readonly property bool artPending: root.anyArtPending(
        root.activeTab === "editorPicks" ? root.editorSections
        : root.activeTab === "forYou" ? root.forYouSections : root.sections)
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

    // ============================ shared components =======================

    // Circular page-control button (Carousel's NavButton).
    // Horizontal album rail (Carousel.slint): header + clipped ListView,
    // page chevrons (per-page step like the Slint paging).
    component AlbumRail: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
        readonly property int step: perPage * 232
        readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)

        QbzSectionHeader {
            title: sectionData.title
            leftEnabled: rail.contentX > 1
            rightEnabled: rail.contentX < maxScroll - 1
            showViewAll: (sectionData.endpoint || "") !== ""
            onPageLeft: rail.contentX = Math.min(0, rail.contentX - step)
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

    // Slim rows mount the shared qml/SlimCard.qml (SlimCard.slint).
    // Popular slim grid (SlimCarousel.slint): 4x3 pages of 12, capped 24.
    component SlimGrid: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: 12
        readonly property int total: Math.min(sectionData.items.length, 2 * perPage)
        readonly property int pageCount: Math.max(1, Math.ceil(total / perPage))
        readonly property real maxScroll: (pageCount - 1) * grid.width

        QbzSectionHeader {
            title: sectionData.title
            leftEnabled: grid.contentX > 1
            rightEnabled: grid.contentX < maxScroll - 1
            showViewAll: (sectionData.endpoint || "") !== ""
            onPageLeft: grid.contentX = Math.max(0, grid.contentX - grid.width)
            onPageRight: grid.contentX = Math.min(maxScroll, grid.contentX + grid.width)
        }
        Item {
            width: parent.width
            height: 3 * 60 + 2 * 8
            clip: true
            Flickable {
                id: grid
                width: parent.width
                height: parent.height
                contentWidth: pageCount * width
                contentHeight: height
                boundsBehavior: Flickable.StopAtBounds
                readonly property real cellWidth: (width - 3 * 8) / 4

                Repeater {
                    model: sectionData.items
                    delegate: SlimCard {
                        readonly property int slot: index % perPage
                        readonly property int pageIdx: Math.floor(index / perPage)
                        visible: index < total
                        width: grid.cellWidth
                        height: 60
                        x: pageIdx * grid.width + (slot % 4) * (grid.cellWidth + 8)
                        y: Math.floor(slot / 4) * (60 + 8)
                        card: modelData
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
        property var sectionsModel: []
        width: parent ? parent.width : 0
        spacing: 40

        Repeater {
            model: sectionsModel
            delegate: Loader {
                required property var modelData
                width: parent ? parent.width : 0
                sourceComponent: modelData.kind === "album" ? albumRailComp
                    : modelData.kind === "playlist" ? playlistRailComp
                    : modelData.kind === "slim" ? slimGridComp
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
                    AlbumRail { sectionData: parent.sectionData }
                }
                Component {
                    id: playlistRailComp
                    Column {
                        property var sectionData: parent.sectionData
                        width: parent ? parent.width : 0
                        spacing: 12
                        Text {
                            text: sectionData.title
                            color: theme.textPrimary
                            font.pixelSize: theme.fontSection
                            font.weight: theme.weightSemibold
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: sectionData.items
                            delegate: PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                            }
                        }
                    }
                }
                Component {
                    id: slimGridComp
                    SlimGrid { sectionData: parent.sectionData }
                }
                Component {
                    id: artistRailComp
                    Column {
                        property var sectionData: parent.sectionData
                        width: parent ? parent.width : 0
                        spacing: 12
                        Text {
                            text: sectionData.title
                            color: theme.textPrimary
                            font.pixelSize: theme.fontSection
                            font.weight: theme.weightSemibold
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: sectionData.items
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
                    tabs: [
                        { "id": "home", "label": QbzSession.tr("Home", QbzSession.trRev) },
                        { "id": "editorPicks", "label": QbzSession.tr("Editor's Picks", QbzSession.trRev) },
                        { "id": "forYou", "label": QbzSession.tr("For You", QbzSession.trRev) },
                        { "id": "recommendations", "label": QbzSession.tr("Recommendations", QbzSession.trRev) },
                    ]
                    activeId: root.activeTab
                    // Data is per-tab JSON (no refetch on switch); scroll
                    // resets to top.
                    onSelected: function (id) {
                        root.activeTab = id
                        homeFlick.contentY = 0
                    }
                }
            }

            // Genre filter + configurator gear — INERT stubs (POC-NOTE).
            Row {
                x: parent.width - width - 32
                y: 25 - height / 2
                height: 32
                spacing: 6
                Rectangle {
                    width: genreRow.width
                    height: 32
                    radius: 6
                    color: genreArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    Row {
                        id: genreRow
                        height: parent.height
                        leftPadding: 12
                        rightPadding: 14
                        spacing: 7
                        QbzIcon {
                            name: "list-filter"
                            width: 14
                            height: 14
                            anchors.verticalCenter: parent.verticalCenter
                            tintName: "secondary"
                        }
                        Text {
                            text: QbzSession.tr("Filter by genre", QbzSession.trRev)
                            color: theme.textSecondary
                            font.pixelSize: 13
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    MouseArea {
                        id: genreArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                    }
                }
                Rectangle {
                    width: 32
                    height: 32
                    radius: 4
                    color: gearArea.containsMouse ? theme.surfaceHover : "transparent"
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
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
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
                    SectionRails { sectionsModel: root.sections }
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
                    SectionRails { sectionsModel: root.editorSections }
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
                    SectionRails { sectionsModel: root.forYouSections }
                }

                // ===== Recommendations (POC-NOTE placeholder) =============
                // The tab follows the Slint gating (showRecommendations pref
                // — ON for this user, so the tab shows); the CONTENT is the
                // external reco engine (crates/qbz/src/external_reco.rs —
                // seeded similar albums, weeklies builders, dismissal
                // stores), which is not ported to the POC.
                Column {
                    visible: root.activeTab === "recommendations"
                    width: parent.width - 64
                    spacing: 10
                    Text {
                        text: QbzSession.tr("Recommendations", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightSemibold
                    }
                    Text {
                        text: "QBZ Qt POC — the external recommendations engine is not ported (external_reco.rs)"
                        color: theme.textMuted
                        font.pixelSize: 13
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
}
