// Discover > Home — QML port of crates/qbz-ui/ui/discover/HomeView.slint
// (+ Carousel / AlbumCard / SlimCarousel / SlimCard / ArtistCarousel /
// PlaylistCarousel / HomeSkeleton / OfflinePlaceholder).
//
// Data: QbzBridge.homeSectionsJson (one JSON document — see bridge.rs),
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
import com.blitzfc.qbz

Rectangle {
    id: root
    color: theme.surfaceMain
    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

    QbzTheme { id: theme }

    // Reparsed whenever Rust republishes the JSON documents (one per
    // Discover tab — phase 13).
    readonly property var sections: JSON.parse(QbzBridge.homeSectionsJson)
    readonly property var editorSections: JSON.parse(QbzBridge.editorSectionsJson)
    readonly property var forYouSections: JSON.parse(QbzBridge.forYouSectionsJson)
    property string activeTab: "home"

    // ============================ shared components =======================

    // Circular page-control button (Carousel's NavButton).
    component NavBtn: Rectangle {
        property string name: ""
        property bool btnEnabled: true
        signal clicked()

        width: 28
        height: 28
        radius: 14
        opacity: btnEnabled ? 1.0 : 0.4
        color: (nbArea.containsMouse && btnEnabled) ? theme.surfaceHover : theme.surfaceElevated
        QbzIcon {
            name: parent.name
            width: 15
            height: 15
            anchors.centerIn: parent
            tintName: parent.btnEnabled ? "primary" : "muted"
        }
        MouseArea {
            id: nbArea
            anchors.fill: parent
            enabled: parent.btnEnabled
            hoverEnabled: true
            cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: parent.clicked()
        }
    }

    // "View all →" header link (Carousel.slint ViewAllLink) — shown when
    // the section carries a discover endpoint. POC-NOTE: the click is
    // INERT (the DiscoverBrowse full-list page is out of scope).
    component ViewAllLink: Rectangle {
        width: linkText.implicitWidth + 16
        height: 26
        radius: 4
        color: vaArea.containsMouse ? theme.surfaceHover : "transparent"
        Text {
            id: linkText
            anchors.centerIn: parent
            text: QbzBridge.tr("View all →")
            color: vaArea.containsMouse ? theme.textPrimary : theme.textSecondary
            font.pixelSize: 14
            font.weight: theme.weightMedium
        }
        MouseArea {
            id: vaArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
        }
    }

    // Section header: title + [View all] + page chevrons (Carousel header
    // metrics — the link sits LEFT of the chevrons, Tauri parity).
    component RailHeader: Item {
        property string title: ""
        property bool leftEnabled: false
        property bool rightEnabled: false
        property bool showViewAll: false
        signal pageLeft()
        signal pageRight()

        width: parent ? parent.width : 0
        height: 28
        Text {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: parent.title
            color: theme.textPrimary
            font.pixelSize: theme.fontSection
            font.weight: theme.weightSemibold
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            ViewAllLink {
                visible: parent.parent.showViewAll
                anchors.verticalCenter: parent.verticalCenter
            }
            NavBtn {
                name: "chevron-left"
                btnEnabled: parent.parent.leftEnabled
                onClicked: parent.parent.pageLeft()
            }
            NavBtn {
                name: "chevron-right"
                btnEnabled: parent.parent.rightEnabled
                onClicked: parent.parent.pageRight()
            }
        }
    }

    // Horizontal album rail (Carousel.slint): header + clipped ListView,
    // page chevrons (per-page step like the Slint paging).
    component AlbumRail: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
        readonly property int step: perPage * 232
        readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)

        RailHeader {
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
                delegate: AlbumCard {
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

    // Slim ranked card (SlimCard.slint): 60px row — rank / 44px thumb /
    // title+subtitle.
    component SlimCard: Rectangle {
        property var card: ({})
        height: 60
        radius: theme.radiusSm
        color: slArea.containsMouse ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 12
            Text {
                visible: card.rank !== ""
                width: 20
                anchors.verticalCenter: parent.verticalCenter
                text: card.rank
                color: theme.textMuted
                font.pixelSize: theme.fontBody
                horizontalAlignment: Text.AlignHCenter
            }
            Rectangle {
                width: 44
                height: 44
                radius: 4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: card.artPath
                    radius: 4
                }
            }
            Column {
                width: parent.width - (card.rank !== "" ? 20 : 0) - 44 - 2 * 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: card.title
                    color: theme.textPrimary
                    font.pixelSize: theme.fontLink
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: card.artist
                    color: theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
        }
        MouseArea {
            id: slArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            // Phase 8: slim rows are albums — click opens the album view.
            onClicked: QbzBridge.openAlbum(card.id)
        }
    }

    // Popular slim grid (SlimCarousel.slint): 4x3 pages of 12, capped 24.
    component SlimGrid: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: 12
        readonly property int total: Math.min(sectionData.items.length, 2 * perPage)
        readonly property int pageCount: Math.max(1, Math.ceil(total / perPage))
        readonly property real maxScroll: (pageCount - 1) * grid.width

        RailHeader {
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
    component PinnedArtistCard: Item {
        property var card: ({})
        width: 200
        height: 246

        Column {
            spacing: 0
            // Art zone: 200x200 rounded-square surface (AlbumCard parity)
            // framing a 190px round portrait (ArtistGridCard).
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                Rectangle {
                    width: 190
                    height: 190
                    radius: 95
                    anchors.centerIn: parent
                    color: theme.surfaceMain
                    clip: true
                    RoundedImage {
                        anchors.fill: parent
                        source: card.artPath || ""
                        radius: 95
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.openArtist(card.id)
                    }
                }
            }
            Item { width: 1; height: 6 }
            Text {
                width: 200
                text: card.title
                color: paNameArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                MouseArea {
                    id: paNameArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(card.id)
                }
            }
        }
    }

    component PinnedPlaylistCard: Item {
        property var card: ({})
        width: 200
        height: 246

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
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: playlist page out of scope — inert.
                }
            }
            Item { width: 1; height: 6 }
            Text {
                width: 200
                text: card.title
                color: theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
            }
            Text {
                width: 200
                visible: card.artist !== ""
                text: card.artist
                color: theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }
    }

    component PinnedRail: Column {
        property var sectionData: ({})
        width: parent ? parent.width : 0
        spacing: 12

        readonly property real maxScroll: Math.max(0, rail.contentWidth - rail.width)
        readonly property int perPage: Math.max(1, Math.floor((rail.width + 32) / 232))
        readonly property int step: perPage * 232

        RailHeader {
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
                        PinnedArtistCard { card: modelData }
                    }
                    Component {
                        id: pPlaylist
                        PinnedPlaylistCard { card: modelData }
                    }
                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData.itemKind === "artist" ? pArtist
                            : modelData.itemKind === "playlist" ? pPlaylist : pAlbum
                    }
                }
            }
        }
    }

    // Playlist card (PlaylistCard.slint): 200x246 — 200px cover + category
    // subtag + title.
    component PlaylistCard: Rectangle {
        property var card: ({})
        width: 200
        height: 246
        color: "transparent"

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
                    source: card.artPath
                    radius: theme.radiusSm
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: playlist page out of scope — inert.
                }
            }
            Item { width: 1; height: 8 }
            Text {
                visible: card.category !== ""
                text: card.category
                color: theme.accent
                font.pixelSize: 10
                font.weight: theme.weightSemibold
                font.letterSpacing: 0.5
                elide: Text.ElideRight
                width: 200
            }
            Item { width: 1; height: 4 }
            Text {
                width: 200
                text: card.title
                color: theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
        }
    }

    // Artist card (ArtistCard.slint): 160x220 — 120px circular art + name.
    component ArtistCard: Rectangle {
        property var card: ({})
        width: 160
        height: 220
        radius: 12
        color: "transparent"

        Column {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 8
            Item { width: 1; height: 24 }
            Rectangle {
                width: 120
                height: 120
                radius: 60
                anchors.horizontalCenter: parent.horizontalCenter
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: card.artPath
                    radius: 60
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(card.id)
                }
            }
            Text {
                width: 128
                anchors.horizontalCenter: parent.horizontalCenter
                text: card.title
                color: theme.textPrimary
                font.pixelSize: 14
                font.weight: theme.weightMedium
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
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
                desc: QbzBridge.tr("Elevate your day with a customized selection of music.")
                c0: "#1e3a8a"; c1: "#6366f1"; c2: "#c084fc"
            }
            MixTile {
                badge: "qobuz"; mixName: "WeeklyQ"
                desc: QbzBridge.tr("Take a weekly journey with a fresh mix every Friday.")
                c0: "#065f46"; c1: "#10b981"; c2: "#fbbf24"
            }
            MixTile {
                badge: "qbz"; mixName: "FavQ"
                desc: QbzBridge.tr("A fresh shuffle from your personal library.")
                c0: "#7f1d1d"; c1: "#ef4444"; c2: "#fb923c"
            }
            MixTile {
                badge: "qbz"; mixName: "TopQ"
                desc: QbzBridge.tr("Discover new music from your most-played playlists.")
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
                            delegate: PlaylistCard { card: modelData }
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
                            height: 220
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: sectionData.items
                            delegate: ArtistCard { card: modelData }
                        }
                    }
                }
                Component {
                    id: mixesRailComp
                    MixesRail { sectionData: parent.sectionData }
                }
                Component {
                    id: recentComp
                    RecentPlaceholder {
                        property var sectionData: parent.sectionData
                        title: sectionData.title
                        hint: sectionData.hint
                    }
                }
            }
        }
    }

    // Recently-played placeholder (HomeView.slint RecentPlaceholder).
    component RecentPlaceholder: Column {
        property string title: ""
        property string hint: ""
        width: parent ? parent.width : 0
        spacing: 10
        Text {
            text: title
            color: theme.textPrimary
            font.pixelSize: 18
            font.weight: theme.weightSemibold
        }
        Text {
            text: hint
            color: theme.textMuted
            font.pixelSize: 13
        }
    }

    // Shimmer block + skeleton rows (HomeSkeleton.slint).
    component Shimmer: Rectangle {
        property bool phase: false
        color: theme.surfaceElevated
        radius: theme.radiusSm
        opacity: phase ? 0.85 : 0.4
        Behavior on opacity { NumberAnimation { duration: 900; easing.type: Easing.InOutQuad } }
    }
    component SkeletonRow: Column {
        id: skelRow
        property bool phase: false
        width: parent ? parent.width : 0
        spacing: 12
        Shimmer { phase: skelRow.phase; width: 180; height: 22 }
        Row {
            spacing: 32
            Repeater {
                model: 5
                delegate: Column {
                    spacing: 8
                    Shimmer { phase: skelRow.phase; width: 200; height: 200 }
                    Shimmer { phase: skelRow.phase; width: 140; height: 14 }
                    Shimmer { phase: skelRow.phase; width: 90; height: 12 }
                }
            }
        }
    }

    // ============================ offline gate ============================
    // (OfflinePlaceholder.slint replica; mounted INSTEAD of the view.)
    Column {
        visible: QbzBridge.offline
        anchors.centerIn: parent
        spacing: 0
        QbzIcon {
            name: "cloud-off"
            width: 56
            height: 56
            anchors.horizontalCenter: parent.horizontalCenter
            tintName: "muted"
        }
        Item { width: 1; height: 18 }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzBridge.tr("You're offline")
            color: theme.textPrimary
            font.pixelSize: theme.fontHeading
            font.weight: theme.weightSemibold
        }
        Item { width: 1; height: 8 }
        Text {
            width: 420
            text: QbzBridge.offlineMode === 2
                ? QbzBridge.tr("Offline mode is enabled. Disable it in Settings to use Qobuz.")
                : QbzBridge.tr("No internet connection. Your local library and downloads keep working.")
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
        // POC-NOTE: induced-offline "Open Settings" button omitted — no
        // Settings view exists in the POC yet.
    }

    // ============================ the view ================================
    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzBridge.offline

        // --- Toolbar (fixed 56px) ---------------------------------------
        Item {
            width: parent.width
            height: 56

            Row {
                // Slint left-controls: x 32 + NavButtons (now a 0px
                // placeholder) + 16px spacing -> the pill starts at 48.
                x: 48
                y: 25 - height / 2
                height: tabPill.height
                spacing: 16

                Rectangle {
                    id: tabPill
                    width: tabRow.width
                    height: tabRow.height
                    color: theme.surfaceElevated
                    radius: 6
                    Row {
                        id: tabRow
                        padding: 3
                        spacing: 4
                        Repeater {
                            model: [
                                { "id": "home", "label": QbzBridge.tr("Home") },
                                { "id": "editorPicks", "label": QbzBridge.tr("Editor's Picks") },
                                { "id": "forYou", "label": QbzBridge.tr("For You") },
                                { "id": "recommendations", "label": QbzBridge.tr("Recommendations") },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                property bool active: root.activeTab === modelData.id
                                width: tabText.implicitWidth + 28
                                height: tabText.implicitHeight + 12
                                radius: 4
                                color: active ? theme.surfaceMain
                                     : tabArea.containsMouse ? theme.surfaceHover : "transparent"
                                Text {
                                    id: tabText
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: parent.active ? theme.textPrimary : theme.textMuted
                                    font.pixelSize: 13
                                    font.weight: theme.weightMedium
                                }
                                MouseArea {
                                    id: tabArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    // Data is per-tab JSON (no refetch on
                                    // switch); scroll resets to top.
                                    onClicked: {
                                        root.activeTab = modelData.id
                                        homeFlick.contentY = 0
                                    }
                                }
                            }
                        }
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
                            text: QbzBridge.tr("Filter by genre")
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

                    // Loading skeleton (HomeSkeleton: two shimmer rows).
                    Column {
                        visible: QbzBridge.homeLoading && root.sections.length === 0
                        width: parent.width
                        spacing: 40
                        property bool phase: false
                        Timer {
                            interval: 900
                            running: parent.visible
                            repeat: true
                            onTriggered: parent.phase = !parent.phase
                        }
                        SkeletonRow { phase: parent.phase }
                        SkeletonRow { phase: parent.phase }
                    }

                    // Error state with retry (the Slint Home has no error
                    // arm; the box mirrors the FavoritesView Retry button).
                    Rectangle {
                        visible: QbzBridge.homeError !== ""
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
                                text: QbzBridge.homeError
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
                                    text: QbzBridge.tr("Retry")
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontLegal
                                }
                                MouseArea {
                                    id: retryArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: QbzBridge.reloadHome()
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
                    SectionRails { sectionsModel: root.editorSections }
                }

                // ===== For You (phase 13) =================================
                Column {
                    visible: root.activeTab === "forYou"
                    width: parent.width - 64
                    spacing: 40
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
                        text: QbzBridge.tr("Recommendations")
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
