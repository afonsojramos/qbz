// Artist detail page — QML port of artist/ArtistPageView.slint.
//
// Header (200px circular portrait, name, bio + Read more, CircleAction
// row: Follow / Radio / Network / ⋯, From-catalog/In-library toggle),
// JUMP TO bar (jump-scroll), Popular Tracks (artwork + album column rows,
// Load more 5→all, play/shuffle-all), Latest release, release sections
// (Albums / EPs & Singles / Live / … in the official order, sort menu,
// per-section Load more paged through the core), Appears On, Playlists,
// Other (collapsed), and the 300px Network sidebar (Network/Magazine
// tabs, LABELS, SIMILAR ARTISTS).
//
// POC-NOTEs: MusicBrainz sidebar sections (Origin/Relationships/
// Discovery), the Magazine tab's stories, blacklist banner, artist Scene,
// Share, Create Collection, radio engines (dropdown inert), multi-select,
// the sticky behavior of the JUMP TO bar (it scrolls with the page).

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

    readonly property var artist: JSON.parse(QbzBridge.artistJson)
    readonly property var topTracks: artist.topTracks || []
    readonly property var appearsOn: artist.appearsOn || []
    readonly property var releaseSections: artist.releaseSections || []
    readonly property var labels: artist.labels || []
    readonly property var similarArtists: artist.similarArtists || []
    readonly property var playlists: artist.playlists || []

    property var coverMap: ({})
    property string activeJumpTab: "popular-tracks"
    property string artistTab: "catalog"
    property bool topTracksExpanded: false
    property bool appearsOnExpanded: false
    property bool otherExpanded: false
    property bool networkOpen: false
    property bool showBio: false
    property string netTab: "network"
    readonly property int preview: 5

    // JUMP TO tabs from the present sections (ArtistState.jump-tabs).
    readonly property var jumpTabs: {
        var tabs = []
        if ((artist.bio || "") !== "") tabs.push({ "id": "about", "label": QbzBridge.tr("About") })
        if (topTracks.length > 0) tabs.push({ "id": "popular-tracks", "label": QbzBridge.tr("Popular Tracks") })
        for (var i = 0; i < releaseSections.length; i++) {
            if (releaseSections[i].releaseType !== "other")
                tabs.push({ "id": releaseSections[i].releaseType, "label": releaseSections[i].title })
        }
        if (appearsOn.length > 0) tabs.push({ "id": "appears-on", "label": QbzBridge.tr("Appears On") })
        return tabs
    }

    Connections {
        target: QbzBridge
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
        function onReleaseSectionReady(releaseType, cardsJson, hasMore) {
            var cards = JSON.parse(cardsJson)
            var sections = root.releaseSections
            for (var i = 0; i < sections.length; i++) {
                if (sections[i].releaseType === releaseType) {
                    var seen = {}
                    for (var j = 0; j < sections[i].cards.length; j++) seen[sections[i].cards[j].id] = true
                    for (j = 0; j < cards.length; j++) {
                        if (!seen[cards[j].id]) sections[i].cards.push(cards[j])
                    }
                    sections[i].hasMore = hasMore
                    break
                }
            }
            root.artistChanged()
        }
    }
    Component.onCompleted: dispatchCovers()
    onArtistChanged: dispatchCovers()
    // Cover dispatch keys off the raw document (artist.artUrl etc.), so
    // re-fire when the parsed value actually changes (same stale race).
    onTopTracksChanged: dispatchCovers()
    onArtistTabChanged: if (artistTab === "library") dispatchLibCovers()
    function dispatchLibCovers() {
        var urls = []
        var items = libraryTab.libItems || []
        for (var i = 0; i < items.length; i++) if (items[i].imageUrl) urls.push(items[i].imageUrl)
        if (urls.length > 0) QbzBridge.sidebarArtworkWindow(JSON.stringify(urls))
    }

    function dispatchCovers() {
        var urls = []
        if (artist.artUrl) urls.push(artist.artUrl)
        var i, j
        for (i = 0; i < topTracks.length; i++) if (topTracks[i].artUrl) urls.push(topTracks[i].artUrl)
        for (i = 0; i < releaseSections.length; i++)
            for (j = 0; j < releaseSections[i].cards.length; j++)
                if (releaseSections[i].cards[j].artUrl) urls.push(releaseSections[i].cards[j].artUrl)
        if (urls.length > 0) QbzBridge.sidebarArtworkWindow(JSON.stringify(urls))
    }

    function scrollToSection(id) {
        root.activeJumpTab = id
        for (var i = 0; i < sectionAnchors.children.length; i++) {
            var c = sectionAnchors.children[i]
            if (c.anchorId === id) {
                flick.contentY = sectionAnchors.y + c.y - 48
                return
            }
        }
    }

    component CircleBtn: Rectangle {
        property string name: ""
        property bool active: false
        signal clicked(var mouse)
        width: 32
        height: 32
        radius: 16
        color: (cbArea.containsMouse || active) ? theme.surfaceHover : theme.surfaceElevated
        border.width: 1.5
        border.color: theme.borderMuted
        QbzIcon {
            name: parent.name
            width: 15
            height: 15
            anchors.centerIn: parent
            tintName: parent.active ? "accent" : "primary"
        }
        MouseArea {
            id: cbArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: function (mouse) { parent.clicked(mouse) }
        }
    }

    // Popular Tracks row (TrackRow with artwork + album column).
    component PopularTrackRow: Rectangle {
        property var row: ({})
        property int rowIndex: 0
        property bool showAlbum: true

        readonly property bool isActive: QbzBridge.npTrackId !== "" && QbzBridge.npTrackId === row.id
        readonly property bool hovered: trArea.containsMouse || favArea.containsMouse || moreArea.containsMouse

        width: parent ? parent.width : 0
        height: 50
        radius: 8
        color: hovered ? "#14ffffff" : (rowIndex % 2 === 1 ? "#07ffffff" : "transparent")

        Rectangle {
            visible: isActive
            x: 2
            y: 7
            width: 3
            height: parent.height - 14
            radius: 1.5
            color: theme.accent
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 14

            // Position number (artwork rows carry it separate from the cover).
            Text {
                visible: showAlbum
                width: 32
                anchors.verticalCenter: parent.verticalCenter
                text: row.number
                color: theme.textMuted
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
            // Cover with hover play overlay.
            Rectangle {
                width: showAlbum ? 36 : 32
                height: showAlbum ? 36 : 28
                anchors.verticalCenter: parent.verticalCenter
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true
                QbzIcon {
                    anchors.centerIn: parent
                    name: "music"
                    width: showAlbum ? 16 : 14
                    height: showAlbum ? 16 : 14
                    tintName: "muted"
                }
                RoundedImage {
                    anchors.fill: parent
                    source: root.coverMap[row.artUrl] || ""
                    radius: theme.radiusSm
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: trArea.containsMouse || isActive ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                QbzIcon {
                    visible: trArea.containsMouse || isActive
                    anchors.centerIn: parent
                    name: isActive && QbzBridge.npPlaying ? "pause" : "play-fill"
                    width: 16
                    height: 16
                    tintName: "primary"
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.playArtistTrack(row.id)
                }
            }
            // Title + artist.
            Column {
                width: parent.width - (showAlbum ? 32 + 36 : 32) - albumCell.width - 70 - 92 - 28 - 28 - 32 - 6 * 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    spacing: 6
                    Text {
                        text: row.title
                        color: theme.textPrimary
                        font.pixelSize: 14
                        font.weight: theme.weightMedium
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, parent.parent.width - (row.explicit ? 22 : 0))
                    }
                    Rectangle {
                        visible: row.explicit
                        width: 16
                        height: 16
                        radius: 3
                        anchors.verticalCenter: parent.verticalCenter
                        color: theme.surfaceElevated
                        Text {
                            anchors.centerIn: parent
                            text: "E"
                            color: theme.textMuted
                            font.pixelSize: 9
                            font.weight: theme.weightSemibold
                        }
                    }
                }
                Text {
                    width: parent.width
                    visible: row.artist !== ""
                    text: row.artist
                    color: theme.textMuted
                    font.pixelSize: 13
                    elide: Text.ElideRight
                }
            }
            // Album column.
            Text {
                id: albumCell
                visible: showAlbum
                width: showAlbum ? 220 : 0
                anchors.verticalCenter: parent.verticalCenter
                text: row.album
                color: row.albumId !== "" && albumLinkArea.containsMouse ? theme.textPrimary : theme.textMuted
                font.pixelSize: 13
                elide: Text.ElideRight
                MouseArea {
                    id: albumLinkArea
                    anchors.fill: parent
                    enabled: row.albumId !== ""
                    hoverEnabled: true
                    cursorShape: row.albumId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: QbzBridge.openAlbum(row.albumId)
                }
            }
            Text {
                width: 70
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: theme.textMuted
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                width: 92
                anchors.verticalCenter: parent.verticalCenter
                text: row.qualityTier === "hires" ? "HI-RES" : (row.qualityTier === "cd" ? "CD" : "")
                color: theme.textMuted
                font.pixelSize: 10
                font.weight: theme.weightBold
                horizontalAlignment: Text.AlignHCenter
            }
            // Favorite (live).
            Rectangle {
                width: 28
                height: 28
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: row.isFavorite ? "heart-filled" : "heart"
                    width: 16
                    height: 16
                    tintName: row.isFavorite ? "favorite" : (favArea.containsMouse ? "primary" : "muted")
                }
                MouseArea {
                    id: favArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        row.isFavorite = !row.isFavorite
                        QbzBridge.libraryToggleFavorite("track", row.id)
                    }
                }
            }
            // Offline download — INERT stub.
            Rectangle {
                width: 28
                height: 28
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                QbzIcon { anchors.centerIn: parent; name: "cloud-download"; width: 16; height: 16; tintName: "muted" }
            }
            // ⋯ (play-only menu for the POC).
            Rectangle {
                width: 32
                height: 32
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { anchors.centerIn: parent; name: "ellipsis"; width: 16; height: 16; tintName: moreArea.containsMouse ? "primary" : "muted" }
                MouseArea {
                    id: moreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: the unified track context menu is out of scope.
                }
            }
        }

        MouseArea {
            id: trArea
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            onDoubleClicked: QbzBridge.playArtistTrack(row.id)
            onClicked: mouse.accepted = false
        }
    }

    // Sidebar link row (SidebarLink).
    component SidebarLink: Rectangle {
        property string label: ""
        property string iconName: "user"
        signal clicked()
        width: parent ? parent.width : 0
        height: 28
        radius: 4
        color: slArea.containsMouse ? theme.surfaceElevated : "transparent"
        Row {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 8
            QbzIcon {
                name: iconName
                width: 12
                height: 12
                anchors.verticalCenter: parent.verticalCenter
                tintName: slArea.containsMouse ? "primary" : "muted"
            }
            Text {
                width: parent.width - 20
                anchors.verticalCenter: parent.verticalCenter
                text: label
                color: slArea.containsMouse ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: slArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Release section (ReleaseGrid).
    component ReleaseSection: Column {
        property var section: ({})
        property string anchorId: ""
        width: parent ? parent.width : 0
        spacing: 12

        Row {
            width: parent.width
            spacing: 12
            Text {
                width: parent.width - seeAll.width - sortBtn.width - 24
                anchors.verticalCenter: parent.verticalCenter
                text: section.title
                color: theme.textPrimary
                font.pixelSize: theme.fontHeading
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
            }
            Text {
                id: seeAll
                anchors.verticalCenter: parent.verticalCenter
                height: 28
                text: QbzBridge.tr("See discography")
                color: seeAllArea.containsMouse ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                MouseArea {
                    id: seeAllArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: dedicated discography page out of scope.
                }
            }
            Rectangle {
                id: sortBtn
                width: sortRow.width
                height: 28
                radius: 5
                anchors.verticalCenter: parent.verticalCenter
                color: sortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                Row {
                    id: sortRow
                    height: parent.height
                    leftPadding: 10
                    rightPadding: 10
                    spacing: 6
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: QbzBridge.tr("Newest")
                        color: theme.textSecondary
                        font.pixelSize: 12
                    }
                    QbzIcon { name: "chevron-down"; width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                }
                MouseArea {
                    id: sortArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // POC-NOTE: per-section sort menu (server re-sort) not wired.
                }
            }
        }

        Grid {
            width: parent.width
            columns: Math.max(1, Math.floor((width + 24) / 224))
            columnSpacing: 24
            rowSpacing: 24
            Repeater {
                model: section.cards
                delegate: AlbumCard {
                    albumId: modelData.id
                    title: modelData.title
                    artist: modelData.artist
                    artistId: modelData.artistId
                    genre: modelData.genre
                    year: modelData.year
                    qualityTier: modelData.qualityTier
                    artSource: root.coverMap[modelData.artUrl] || ""
                    isFavorite: false
                }
            }
        }

        Row {
            visible: section.hasMore
            width: parent.width
            Item { width: (parent.width - loadMoreBtn.width) / 2; height: 1 }
            Rectangle {
                id: loadMoreBtn
                width: loadMoreText.implicitWidth + 24
                height: 28
                color: "transparent"
                Text {
                    id: loadMoreText
                    anchors.centerIn: parent
                    text: QbzBridge.tr("Load more")
                    color: loadMoreArea.containsMouse ? theme.textPrimary : theme.textSecondary
                    font.pixelSize: 13
                }
                MouseArea {
                    id: loadMoreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.loadReleaseSection(artist.id, section.releaseType, section.cards.length)
                }
            }
            Item { width: (parent.width - loadMoreBtn.width) / 2; height: 1 }
        }
    }

    // ============================ the page ================================
    Flickable {
        id: flick
        anchors.fill: parent
        anchors.rightMargin: root.networkOpen ? 300 : 0
        clip: true
        contentWidth: width
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        Behavior on anchors.rightMargin { NumberAnimation { duration: 160; easing.type: Easing.InOutQuad } }

        Column {
            id: page
            width: parent.width
            leftPadding: 32
            rightPadding: 32
            topPadding: 11
            bottomPadding: 100
            spacing: 0

            Item { width: 1; height: 22 }

            // --- Artist header ------------------------------------------
            Row {
                width: parent.width - 64
                spacing: 32

                // Circular portrait (rounded Rectangle + clip round-clips
                // on this Qt build — verified against the phase-3 circles).
                Rectangle {
                    width: 200
                    height: 200
                    radius: 100
                    color: theme.surfaceElevated
                    clip: true
                    RoundedImage {
                        anchors.fill: parent
                        source: root.coverMap[artist.artUrl] || ""
                        radius: 100
                    }
                }

                Column {
                    width: parent.width - 200 - 32
                    anchors.top: parent.top
                    anchors.topMargin: 8
                    spacing: 0

                    Text {
                        width: parent.width
                        text: artist.name || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }

                    Item { visible: (artist.bio || "") !== ""; width: 1; height: 12 }
                    Text {
                        visible: (artist.bio || "") !== ""
                        width: parent.width
                        text: artist.bioShort || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                        wrapMode: Text.WordWrap
                    }
                    Item { visible: artist.bioTruncated === true; width: 1; height: 4 }
                    Text {
                        visible: artist.bioTruncated === true
                        text: QbzBridge.tr("Read more")
                        color: readMoreArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: theme.fontLegal
                        MouseArea {
                            id: readMoreArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.showBio = true
                        }
                    }

                    Item { width: 1; height: 18 }
                    // Action row.
                    Row {
                        width: parent.width
                        spacing: 12
                        CircleBtn {
                            name: artist.isFollowing ? "heart-filled" : "heart"
                            active: artist.isFollowing === true
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: {
                                artist.isFollowing = !artist.isFollowing
                                QbzBridge.libraryToggleFavorite("artist", artist.id)
                            }
                        }
                        CircleBtn {
                            id: radioBtn
                            name: "radio"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: radioPopup.openBelowRight(radioBtn)
                        }
                        CircleBtn {
                            name: "element-connect"
                            active: root.networkOpen
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: root.networkOpen = !root.networkOpen
                        }
                        CircleBtn {
                            id: overflowBtn
                            name: "ellipsis"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) { overflowMenu.openAtCursor(overflowBtn, mouse.x, mouse.y) }
                        }
                        Item { width: parent.width - 4 * 32 - 4 * 12 - segTabs.width; height: 1 }
                        // From catalog / In library (webplayer parity).
                        Rectangle {
                            id: segTabs
                            visible: (artist.libraryCount || 0) > 0
                            width: segRow.width
                            height: segRow.height
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.surfaceElevated
                            radius: 6
                            Row {
                                id: segRow
                                padding: 3
                                spacing: 4
                                Repeater {
                                    model: [
                                        { "id": "catalog", "label": QbzBridge.tr("From catalog"), "count": 0 },
                                        { "id": "library", "label": QbzBridge.tr("In library"), "count": artist.libraryCount || 0 },
                                    ]
                                    delegate: Rectangle {
                                        required property var modelData
                                        property bool active: root.artistTab === modelData.id
                                        width: segTabRow.implicitWidth
                                        height: segTabRow.implicitHeight
                                        radius: 4
                                        color: active ? theme.surfaceMain
                                             : segTabArea.containsMouse ? theme.surfaceHover : "transparent"
                                        Row {
                                            id: segTabRow
                                            leftPadding: 12
                                            rightPadding: parent && parent.parent.modelData.count > 0 ? 8 : 12
                                            topPadding: 6
                                            bottomPadding: 6
                                            spacing: 7
                                            Text {
                                                text: modelData.label
                                                color: parent.parent.active ? theme.textPrimary : theme.textMuted
                                                font.pixelSize: theme.fontLegal
                                                font.weight: theme.weightMedium
                                            }
                                            Rectangle {
                                                visible: modelData.count > 0
                                                width: Math.max(18, segCountText.implicitWidth + 10)
                                                height: 16
                                                radius: 8
                                                color: parent.parent.active ? "#26ffffff" : "#14ffffff"
                                                Text {
                                                    id: segCountText
                                                    anchors.centerIn: parent
                                                    text: modelData.count
                                                    color: parent.parent.active ? theme.textPrimary : theme.textSecondary
                                                    font.pixelSize: 11
                                                    font.weight: theme.weightMedium
                                                }
                                            }
                                        }
                                        MouseArea {
                                            id: segTabArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: root.artistTab = modelData.id
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Item { width: 1; height: 20 }

            // --- JUMP TO bar (inline; sticky is POC-NOTE) -----------------
            Rectangle {
                width: parent.width - 64
                height: 44
                color: theme.surfaceMain
                Row {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 16
                    Repeater {
                        model: root.jumpTabs
                        delegate: Column {
                            required property var modelData
                            spacing: 0
                            Text {
                                text: modelData.label
                                color: root.activeJumpTab === modelData.id ? theme.textPrimary
                                     : jumpTabArea.containsMouse ? theme.textSecondary : theme.textMuted
                                font.pixelSize: 13
                                font.weight: root.activeJumpTab === modelData.id ? theme.weightSemibold : theme.weightMedium
                                MouseArea {
                                    id: jumpTabArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.scrollToSection(modelData.id)
                                }
                            }
                            Rectangle {
                                visible: root.activeJumpTab === modelData.id
                                width: parent.width
                                height: 2
                                radius: 1
                                color: theme.accent
                            }
                        }
                    }
                }
            }

            // --- Loading -------------------------------------------------
            Item {
                visible: QbzBridge.artistLoading && topTracks.length === 0
                width: parent.width - 64
                height: 280
                Column {
                    anchors.centerIn: parent
                    spacing: 18
                    QbzSpinner { size: 36; anchors.horizontalCenter: parent.horizontalCenter }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzBridge.tr("Loading artist…")
                        color: theme.textMuted
                        font.pixelSize: 13
                    }
                }
            }

            // ================= Catalog tab ================================
            Column {
                id: sectionAnchors
                visible: root.artistTab === "catalog" && !QbzBridge.artistLoading
                width: parent.width - 64
                spacing: 0

                // --- Popular Tracks -------------------------------------
                Row {
                    property string anchorId: "popular-tracks"
                    visible: topTracks.length > 0
                    width: parent.width
                    spacing: 12
                    Text {
                        width: parent.width - 44 - 32 - 32 - 3 * 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: QbzBridge.tr("Popular Tracks")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                    }
                    Rectangle {
                        width: 44
                        height: 44
                        radius: 22
                        anchors.verticalCenter: parent.verticalCenter
                        color: playTopArea.containsMouse ? theme.accentHover : theme.accent
                        QbzIcon { anchors.centerIn: parent; name: "play-fill"; width: 19; height: 19; tintName: "primary" }
                        MouseArea {
                            id: playTopArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: QbzBridge.playArtistTop(false)
                        }
                    }
                    CircleBtn {
                        name: "square-check-big"
                        anchors.verticalCenter: parent.verticalCenter
                        // POC-NOTE: multi-select out of scope.
                    }
                    CircleBtn {
                        id: topMenuBtn
                        name: "ellipsis"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: function (mouse) { topMenu.openAtCursor(topMenuBtn, mouse.x, mouse.y) }
                    }
                }
                Item { visible: topTracks.length > 0; width: 1; height: 10 }

                Repeater {
                    model: topTracks.length
                    delegate: PopularTrackRow {
                        visible: root.topTracksExpanded || index < root.preview
                        height: visible ? 50 : 0
                        row: topTracks[index]
                        rowIndex: index
                        showAlbum: true
                    }
                }
                Item { visible: topTracks.length > root.preview; width: 1; height: 4 }
                Rectangle {
                    visible: topTracks.length > root.preview
                    width: parent.width
                    height: 28
                    color: "transparent"
                    Text {
                        anchors.centerIn: parent
                        text: root.topTracksExpanded ? QbzBridge.tr("View less") : QbzBridge.tr("Load more")
                        color: loadMoreTopArea.containsMouse ? theme.textPrimary : theme.textSecondary
                        font.pixelSize: 13
                        MouseArea {
                            id: loadMoreTopArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.topTracksExpanded = !root.topTracksExpanded
                        }
                    }
                }

                // --- Latest release --------------------------------------
                Column {
                    property string anchorId: "about"
                    visible: !!artist.lastRelease
                    width: parent.width
                    spacing: 12
                    Item { width: 1; height: 32 }
                    Text {
                        text: QbzBridge.tr("Latest release")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                    }
                    AlbumCard {
                        albumId: artist.lastRelease ? artist.lastRelease.id : ""
                        title: artist.lastRelease ? artist.lastRelease.title : ""
                        artist: artist.lastRelease ? artist.lastRelease.artist : ""
                        artistId: artist.lastRelease ? artist.lastRelease.artistId : ""
                        genre: artist.lastRelease ? artist.lastRelease.genre : ""
                        year: artist.lastRelease ? artist.lastRelease.year : ""
                        qualityTier: artist.lastRelease ? artist.lastRelease.qualityTier : ""
                        artSource: artist.lastRelease ? (root.coverMap[artist.lastRelease.artUrl] || "") : ""
                        isFavorite: false
                    }
                }

                // --- Release sections ------------------------------------
                Repeater {
                    model: releaseSections
                    delegate: Column {
                        required property var modelData
                        width: parent ? parent.width : 0
                        spacing: 0
                        visible: modelData.releaseType !== "other"
                        Item { width: 1; height: 32 }
                        ReleaseSection { section: modelData; anchorId: modelData.releaseType }
                    }
                }

                // --- Appears On -------------------------------------------
                Column {
                    property string anchorId: "appears-on"
                    visible: appearsOn.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    Text {
                        text: QbzBridge.tr("Appears On")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                    }
                    Item { width: 1; height: 10 }
                    Repeater {
                        model: appearsOn.length
                        delegate: PopularTrackRow {
                            visible: root.appearsOnExpanded || index < root.preview
                            height: visible ? 50 : 0
                            row: appearsOn[index]
                            rowIndex: index
                            showAlbum: false
                        }
                    }
                    Item { visible: appearsOn.length > root.preview; width: 1; height: 4 }
                    Rectangle {
                        visible: appearsOn.length > root.preview
                        width: parent.width
                        height: 28
                        color: "transparent"
                        Text {
                            anchors.centerIn: parent
                            text: root.appearsOnExpanded ? QbzBridge.tr("View less") : QbzBridge.tr("Load more")
                            color: loadMoreAppArea.containsMouse ? theme.textPrimary : theme.textSecondary
                            font.pixelSize: 13
                            MouseArea {
                                id: loadMoreAppArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.appearsOnExpanded = !root.appearsOnExpanded
                            }
                        }
                    }
                }

                // --- Playlists --------------------------------------------
                Column {
                    visible: playlists.length > 0
                    width: parent.width
                    spacing: 12
                    Item { width: 1; height: 32 }
                    Text {
                        text: QbzBridge.tr("Playlists")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                    }
                    ListView {
                        width: parent.width
                        height: 246
                        orientation: ListView.Horizontal
                        spacing: 32
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        model: playlists
                        delegate: Rectangle {
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
                                        source: root.coverMap[modelData.artUrl] || ""
                                        radius: theme.radiusSm
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        // POC-NOTE: no playlist view yet.
                                    }
                                }
                                Item { width: 1; height: 6 }
                                Text {
                                    width: 200
                                    text: modelData.title
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontBody - 2
                                    font.weight: theme.weightMedium
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: 200
                                    text: modelData.subtitle
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLink - 1
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }

                // --- Other (collapsed) ------------------------------------
                Repeater {
                    model: releaseSections
                    delegate: Column {
                        required property var modelData
                        width: parent ? parent.width : 0
                        spacing: 0
                        visible: modelData.releaseType === "other"
                        Item { width: 1; height: 32 }
                        Row {
                            width: parent.width
                            spacing: 8
                            Text {
                                width: parent.width - otherToggle.implicitWidth - 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.title
                                color: theme.textPrimary
                                font.pixelSize: theme.fontHeading
                                font.weight: theme.weightSemibold
                            }
                            Text {
                                id: otherToggle
                                anchors.verticalCenter: parent.verticalCenter
                                text: root.otherExpanded ? QbzBridge.tr("Hide") : QbzBridge.tr("Show")
                                color: otherToggleArea.containsMouse ? theme.textPrimary : theme.textSecondary
                                font.pixelSize: 13
                                MouseArea {
                                    id: otherToggleArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.otherExpanded = !root.otherExpanded
                                }
                            }
                        }
                        Item { visible: root.otherExpanded; width: 1; height: 12 }
                        Grid {
                            visible: root.otherExpanded
                            width: parent.width
                            columns: Math.max(1, Math.floor((width + 24) / 224))
                            columnSpacing: 24
                            rowSpacing: 24
                            Repeater {
                                model: modelData.cards
                                delegate: AlbumCard {
                                    albumId: modelData.id
                                    title: modelData.title
                                    artist: modelData.artist
                                    artistId: modelData.artistId
                                    genre: modelData.genre
                                    year: modelData.year
                                    qualityTier: modelData.qualityTier
                                    artSource: root.coverMap[modelData.artUrl] || ""
                                    isFavorite: false
                                }
                            }
                        }
                    }
                }
            }

            // ================= In library tab =============================
            Column {
                id: libraryTab
                visible: root.artistTab === "library" && !QbzBridge.artistLoading
                width: parent.width - 64
                spacing: 0
                readonly property var libItems: {
                    var out = []
                    var feed = libraryFeed()
                    for (var i = 0; i < feed.length; i++) {
                        if (feed[i].artistId === artist.id && (feed[i].kind === "track" || feed[i].kind === "album"))
                            out.push(feed[i])
                    }
                    return out
                }
                readonly property var libAlbums: libItems.filter(function (x) { return x.kind === "album" })
                readonly property var libTracks: libItems.filter(function (x) { return x.kind === "track" })

                Text {
                    visible: libTracks.length > 0
                    text: QbzBridge.tr("Tracks")
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                }
                Item { visible: libTracks.length > 0; width: 1; height: 10 }
                Repeater {
                    model: libraryTab.libTracks
                    delegate: PopularTrackRow {
                        row: ({
                            "id": modelData.id, "number": index + 1, "title": modelData.title,
                            "artist": modelData.artist, "artistId": modelData.artistId,
                            "album": modelData.album, "albumId": modelData.albumId,
                            "duration": modelData.duration, "qualityTier": modelData.qualityTier,
                            "explicit": modelData.explicit, "artUrl": modelData.imageUrl,
                            "isFavorite": modelData.isFavorite,
                        })
                        rowIndex: index
                        showAlbum: true
                    }
                }
                Item { visible: libraryTab.libAlbums.length > 0; width: 1; height: 24 }
                Text {
                    visible: libraryTab.libAlbums.length > 0
                    text: QbzBridge.tr("Albums")
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                }
                Item { visible: libraryTab.libAlbums.length > 0; width: 1; height: 10 }
                Grid {
                    visible: libraryTab.libAlbums.length > 0
                    width: parent.width
                    columns: Math.max(1, Math.floor((width + 24) / 224))
                    columnSpacing: 24
                    rowSpacing: 24
                    Repeater {
                        model: libraryTab.libAlbums
                        delegate: AlbumCard {
                            albumId: modelData.id
                            title: modelData.title
                            artist: modelData.artist
                            artistId: modelData.artistId
                            genre: modelData.genre
                            year: modelData.year
                            qualityTier: modelData.qualityTier
                            artSource: root.coverMap[modelData.imageUrl] || ""
                            isFavorite: modelData.isFavorite
                        }
                    }
                }
            }
        }

        QbzScrollBar {
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            target: flick
        }
    }

    // Library feed access (the phase-5 document, parsed in LibraryView).
    function libraryFeed() {
        return JSON.parse(QbzBridge.libraryJson)
    }

    // --- Network sidebar (300px, surface-card + 1px left border) ---------
    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.networkOpen ? 300 : 0
        clip: true
        color: theme.surfaceCard
        Behavior on width { NumberAnimation { duration: 160; easing.type: Easing.InOutQuad } }

        Rectangle { anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 1; color: theme.borderSubtle }

        Column {
            anchors.fill: parent
            spacing: 0

            // Header: Network / Magazine tabs + close.
            Item {
                width: parent.width
                height: 44
                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 14
                    Repeater {
                        model: [
                            { "id": "network", "label": QbzBridge.tr("Network") },
                            { "id": "magazine", "label": QbzBridge.tr("Magazine") },
                        ]
                        delegate: Column {
                            required property var modelData
                            spacing: 0
                            Text {
                                text: modelData.label
                                color: root.netTab === modelData.id ? theme.textPrimary : theme.textMuted
                                font.pixelSize: 12
                                font.weight: theme.weightSemibold
                                font.letterSpacing: 0.8
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.netTab = modelData.id
                                }
                            }
                            Rectangle {
                                visible: root.netTab === modelData.id
                                width: parent.width
                                height: 2
                                radius: 1
                                color: theme.accent
                            }
                        }
                    }
                }
                Rectangle {
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: 28
                    height: 28
                    radius: 6
                    color: netCloseArea.containsMouse ? theme.surfaceElevated : "transparent"
                    QbzIcon {
                        anchors.centerIn: parent
                        name: "panel-right-close"
                        width: 18
                        height: 18
                        tintName: netCloseArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: netCloseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.networkOpen = false
                    }
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }

            // Network tab body.
            Flickable {
                visible: root.netTab === "network"
                width: parent.width
                height: parent.height - 45
                clip: true
                contentWidth: width
                contentHeight: netBody.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: netBody
                    width: parent.width
                    topPadding: 4
                    bottomPadding: 12
                    spacing: 0

                    // LABELS.
                    Column {
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 6
                        spacing: 4
                        Text {
                            text: QbzBridge.tr("LABELS")
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }
                        Text {
                            visible: labels.length === 0
                            text: QbzBridge.tr("No label info")
                            color: theme.textMuted
                            font.pixelSize: 12
                        }
                        Repeater {
                            model: labels
                            delegate: SidebarLink {
                                label: modelData.name
                                iconName: "disc"
                                // POC-NOTE: no label view yet.
                            }
                        }
                    }
                    // SIMILAR ARTISTS.
                    Column {
                        visible: similarArtists.length > 0 || QbzBridge.artistLoading
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 6
                        spacing: 4
                        Text {
                            text: QbzBridge.tr("SIMILAR ARTISTS")
                            color: theme.textMuted
                            font.pixelSize: 11
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 0.5
                        }
                        Text {
                            visible: similarArtists.length === 0 && QbzBridge.artistLoading
                            text: QbzBridge.tr("Loading…")
                            color: theme.textMuted
                            font.pixelSize: 12
                        }
                        Repeater {
                            model: similarArtists
                            delegate: SidebarLink {
                                label: modelData.name
                                iconName: "user"
                                onClicked: QbzBridge.openArtist(modelData.id)
                            }
                        }
                    }
                }
            }

            // Magazine tab body — POC-NOTE placeholder (the Stories CMS is
            // a separate subsystem).
            Item {
                visible: root.netTab === "magazine"
                width: parent.width
                height: parent.height - 45
                Text {
                    anchors.centerIn: parent
                    width: parent.width - 28
                    text: "QBZ Qt POC — the Magazine feed is not wired yet"
                    color: theme.textMuted
                    font.pixelSize: 12
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    // --- Radio dropdown (INERT items — POC-NOTE: radio engines) ----------
    QbzContextMenu {
        id: radioPopup
        menuWidth: 180
            Repeater {
                model: [QbzBridge.tr("QBZ Radio"), QbzBridge.tr("Qobuz Radio")]
                delegate: Rectangle {
                    required property string modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    color: radioOptArea.containsMouse ? theme.surfaceHover : "transparent"
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        spacing: 8
                        QbzIcon { name: "radio"; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                        Text {
                            height: parent.height
                            text: modelData
                            color: theme.textSecondary
                            font.pixelSize: 13
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                    MouseArea {
                        id: radioOptArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: radioPopup.close()
                    }
                }
            }
        }

    // --- ⋯ overflow menu ---------------------------------------------------
    QbzContextMenu {
        id: overflowMenu
        menuWidth: 224
            Repeater {
                model: [
                    { "label": QbzBridge.tr("Create Artist Collection"), "icon": "library-big", "action": "stub" },
                    { "label": QbzBridge.tr("Artist Scene"), "icon": "map-pin", "action": "stub" },
                    { "label": QbzBridge.tr("Share"), "icon": "link", "action": "stub" },
                    { "label": artist.isPinned ? QbzBridge.tr("Unpin") : QbzBridge.tr("Pin"), "icon": artist.isPinned ? "pin-filled" : "pin", "action": "pin" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    color: omiArea.containsMouse ? theme.surfaceHover : "transparent"
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        spacing: 8
                        QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                        Text {
                            height: parent.height
                            width: parent.width - 23
                            text: modelData.label
                            color: theme.textSecondary
                            font.pixelSize: 13
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                    }
                    MouseArea {
                        id: omiArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            overflowMenu.close()
                            if (modelData.action === "pin") {
                                artist.isPinned = !artist.isPinned
                                QbzBridge.togglePin("artist", artist.id, artist.name, "", artist.artUrl)
                            }
                        }
                    }
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Rectangle {
                width: parent.width
                height: 33
                radius: 5
                color: blArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    spacing: 8
                    QbzIcon { name: "blind-eye"; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    Text {
                        height: parent.height
                        text: QbzBridge.tr("Blacklist artist")
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                    }
                }
                MouseArea {
                    id: blArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: overflowMenu.close()
                }
            }
        }

    // --- Popular Tracks ⋯ menu ---------------------------------------------
    QbzContextMenu {
        id: topMenu
        menuWidth: 224
            Repeater {
                model: [
                    { "label": QbzBridge.tr("Play all next"), "icon": "list-start", "action": "next-all" },
                    { "label": QbzBridge.tr("Add all to queue"), "icon": "list-end", "action": "queue-all" },
                    { "label": QbzBridge.tr("Shuffle all"), "icon": "shuffle", "action": "shuffle-all" },
                    { "label": QbzBridge.tr("Add all to playlist"), "icon": "list-music", "action": "playlist-all" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    color: tmiArea.containsMouse ? theme.surfaceHover : "transparent"
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        spacing: 8
                        QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                        Text {
                            height: parent.height
                            width: parent.width - 23
                            text: modelData.label
                            color: theme.textSecondary
                            font.pixelSize: 13
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                    }
                    MouseArea {
                        id: tmiArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            topMenu.close()
                            var a = modelData.action
                            if (a === "shuffle-all") QbzBridge.playArtistTop(true)
                            else if (a === "next-all") QbzBridge.playArtistTop(false)
                            else if (a === "queue-all") QbzBridge.enqueueArtistTop()
                            // playlist-all: inert (no picker) — POC-NOTE.
                        }
                    }
                }
            }
        }

    // --- Full-bio modal ----------------------------------------------------
    Rectangle {
        visible: root.showBio
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: root.showBio = false
        }
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(root.width - 80, 560)
            height: Math.min(root.height - 120, 460)
            radius: theme.radiusMd
            color: theme.surfaceCard
            border.width: 1
            border.color: theme.borderSubtle
            MouseArea { anchors.fill: parent }
            Column {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 14
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - 28
                        text: artist.name || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Rectangle {
                        width: 28
                        height: 28
                        color: bioCloseArea.containsMouse ? theme.surfaceHover : "transparent"
                        radius: 6
                        QbzIcon {
                            anchors.centerIn: parent
                            name: "x"
                            width: 18
                            height: 18
                            tintName: bioCloseArea.containsMouse ? "primary" : "muted"
                        }
                        MouseArea {
                            id: bioCloseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.showBio = false
                        }
                    }
                }
                Flickable {
                    width: parent.width
                    height: parent.height - 42
                    clip: true
                    contentWidth: width
                    contentHeight: bioText.implicitHeight
                    Text {
                        id: bioText
                        width: parent.width
                        text: artist.bio || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }
}
