// Discover > Home — QML port of crates/qbz-ui/ui/discover/HomeView.slint
// (+ Carousel / AlbumCard / SlimCarousel / SlimCard / ArtistCarousel /
// PlaylistCarousel / HomeSkeleton / OfflinePlaceholder).
//
// Data: QbzBridge.homeSectionsJson (one JSON document — see bridge.rs),
// published by src/home_qt.rs; artwork file:// paths resolve through the
// qbz-cache image cache. Section kinds: "album" | "playlist" | "slim" |
// "artists" | "recentPlaceholder".
//
// POC-NOTEs:
// - The genre filter + section-configurator gear are INERT visual stubs
//  (out of scope).
// - Editor's Picks / For You / Recommendations tabs render a placeholder
//  page with the exact header copy; only "home" is real.
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

    QbzTheme { id: theme }

    // Reparsed whenever Rust republishes the JSON document.
    readonly property var sections: JSON.parse(QbzBridge.homeSectionsJson)
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

    // Section header: title + page chevrons (Carousel header metrics).
    component RailHeader: Item {
        property string title: ""
        property bool leftEnabled: false
        property bool rightEnabled: false
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

    // Album card (AlbumCard.slint): 200x246 — 200px art (radius 8) + 6px +
    // 40px title/artist row with the icon-only quality badge.
    component AlbumCard: Rectangle {
        property var card: ({})
        width: 200
        height: 246
        color: "transparent"

        readonly property bool overlayOn: artArea.containsMouse

        Column {
            spacing: 0
            // --- Artwork + hover overlay --------------------------------
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true

                Image {
                    anchors.fill: parent
                    source: card.artPath
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                // Hover scrim.
                Rectangle {
                    anchors.fill: parent
                    color: "#000000"
                    opacity: overlayOn ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                // Hover meta — genre + year, top-left.
                Column {
                    x: 12
                    y: 12
                    spacing: 2
                    opacity: overlayOn ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                    Text {
                        visible: card.genre !== ""
                        text: card.genre
                        height: 20
                        color: "#ebffffff"
                        font.pixelSize: 13
                        font.weight: theme.weightBold
                        verticalAlignment: Text.AlignVCenter
                    }
                    Text {
                        visible: card.year !== ""
                        text: card.year
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
                    // Phase 4: a card click plays the album (the album page
                    // itself is out of scope — POC-NOTE).
                    onClicked: QbzBridge.playAlbum(card.id)
                }
                // Hover action buttons — favorite / play / more (INERT;
                // POC-NOTE: playback + favorites land in later phases).
                Row {
                    x: 0
                    y: 120
                    width: 200
                    height: 44
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 12
                    opacity: overlayOn ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }

                    Item { width: (parent.width - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                    Rectangle {
                        width: 36
                        height: 36
                        radius: 18
                        anchors.verticalCenter: parent.verticalCenter
                        color: favArea.containsMouse ? "#3dffffff" : "#24ffffff"
                        border.width: 1.5
                        border.color: "#ccffffff"
                        QbzIcon {
                            name: "heart"
                            width: 16
                            height: 16
                            anchors.centerIn: parent
                            tintName: "primary"
                        }
                        MouseArea { id: favArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor }
                    }
                    Rectangle {
                        width: 44
                        height: 44
                        radius: 22
                        color: playOvArea.containsMouse ? "#d6ffffff" : "#ffffff"
                        // Black glyph on the white disc (Slint tints #000)
                        // — the "black" baked variant.
                        QbzIcon {
                            name: "play-fill"
                            width: 18
                            height: 18
                            anchors.centerIn: parent
                            tintName: "black"
                        }
                        MouseArea {
                            id: playOvArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: QbzBridge.playAlbum(card.id)
                        }
                    }
                    Rectangle {
                        width: 36
                        height: 36
                        radius: 18
                        anchors.verticalCenter: parent.verticalCenter
                        color: moreArea.containsMouse ? "#3dffffff" : "#24ffffff"
                        border.width: 1.5
                        border.color: "#ccffffff"
                        QbzIcon {
                            name: "ellipsis"
                            width: 16
                            height: 16
                            anchors.centerIn: parent
                            tintName: "primary"
                        }
                        MouseArea { id: moreArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor }
                    }
                    Item { width: (parent.width - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                }
                // Award ribbon — content-width, capped at the card width.
                Rectangle {
                    visible: card.ribbon !== ""
                    x: 0
                    y: parent.height - height - 8
                    height: 20
                    width: Math.min(ribbonRow.width, 200)
                    color: card.ribbonKind === "press" ? "#d49511" : "#e0000000"
                    topRightRadius: 3
                    bottomRightRadius: 3
                    clip: true
                    Rectangle {
                        width: card.ribbonKind === "press" ? 0 : 3
                        height: parent.height
                        color: card.ribbonKind === "qobuzissime" ? "#8b5cf6" : "#eab308"
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
                            text: card.ribbon
                            color: card.ribbonKind === "press" ? "#1f1407" : "#ffffff"
                            font.pixelSize: 9
                            font.weight: theme.weightSemibold
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                    }
                }
            }
            Item { width: 1; height: 6 }
            // --- Title / artist + quality badge --------------------------
            Row {
                width: 200
                height: 40
                spacing: theme.spacingSm
                Column {
                    width: parent.width - (qualityBadge.visible ? qualityBadge.width + theme.spacingSm : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: parent.width
                        height: 20
                        text: card.title
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
                        }
                    }
                    Text {
                        width: parent.width
                        height: 18
                        text: card.artist
                        color: card.artistId !== "" && artistArea.containsMouse
                            ? theme.textPrimary : theme.textMuted
                        font.pixelSize: theme.fontLink - 1
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        MouseArea {
                            id: artistArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: card.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        }
                    }
                }
                // Icon-only quality badge (QualityBadge.slint).
                Item {
                    id: qualityBadge
                    visible: card.qualityTier !== ""
                    width: card.qualityTier === "hires" ? 42 : 30
                    height: 30
                    anchors.verticalCenter: parent.verticalCenter
                    Image {
                        visible: card.qualityTier === "hires"
                        source: "assets/hi-res.svg"
                        width: 42
                        height: 28
                        anchors.centerIn: parent
                        sourceSize: Qt.size(84, 56)
                        fillMode: Image.PreserveAspectFit
                    }
                    Rectangle {
                        visible: card.qualityTier === "cd"
                        width: 30
                        height: 30
                        radius: 3
                        color: theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle
                        QbzIcon {
                            name: "cd"
                            width: 16
                            height: 16
                            anchors.centerIn: parent
                            tintName: "muted"
                        }
                    }
                }
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
            onPageLeft: rail.contentX = Math.min(0, rail.contentX - step)
            onPageRight: rail.contentX = Math.min(maxScroll, rail.contentX + step)
        }
        ListView {
            id: rail
            width: parent.width
            height: 246
            orientation: ListView.Horizontal
            spacing: 32
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sectionData.items
            delegate: AlbumCard { card: modelData }
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
                Image {
                    anchors.fill: parent
                    source: card.artPath
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
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
            // Phase 4: slim rows are albums — click plays (page out of scope).
            onClicked: QbzBridge.playAlbum(card.id)
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
                Image {
                    anchors.fill: parent
                    source: card.artPath
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
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
                Image {
                    anchors.fill: parent
                    source: card.artPath
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: artist page out of scope — inert.
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
                x: 32
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
                                    onClicked: root.activeTab = modelData.id
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
        Flickable {
            id: homeFlick
            width: parent.width
            height: parent.height - 57
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
                    Repeater {
                        model: root.sections
                        delegate: Loader {
                            required property var modelData
                            width: homeTab.width
                            sourceComponent: modelData.kind === "album" ? albumRailComp
                                : modelData.kind === "playlist" ? playlistRailComp
                                : modelData.kind === "slim" ? slimGridComp
                                : modelData.kind === "artists" ? artistRailComp
                                : recentComp
                            property var sectionData: modelData

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

                // ===== Placeholder tabs (POC-NOTE) ========================
                Column {
                    visible: root.activeTab !== "home"
                    width: parent.width - 64
                    spacing: 10
                    Text {
                        text: root.activeTab === "editorPicks" ? QbzBridge.tr("Editor's Picks")
                            : root.activeTab === "forYou" ? QbzBridge.tr("For You")
                            : QbzBridge.tr("Recommendations")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightSemibold
                    }
                    Text {
                        text: "QBZ Qt POC — this Discover tab lands in a later phase"
                        color: theme.textMuted
                        font.pixelSize: 13
                    }
                }
            }
        }
    }
}
