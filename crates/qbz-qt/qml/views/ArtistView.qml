// Artist detail page — QML port of artist/ArtistPageView.slint.
//
// Header (200px circular portrait, name, bio + Read more, CircleAction
// row: Follow / Radio / Network / ⋯, From-catalog/In-library toggle),
// JUMP TO bar (jump-scroll), Popular Tracks (artwork + album column rows,
// Load more 5→all, play/shuffle-all), Latest release, release sections
// (Albums / EPs & Singles / Live / … in the official order, sort menu,
// per-section Load more paged through the core), Appears On, Playlists,
// Other (collapsed), and the 300px Network sidebar (Network/Magazine
// tabs, ORIGIN, LABELS, SIMILAR ARTISTS, RELATIONSHIPS, YOU MAY ALSO LIKE,
// and the Magazine story teasers).
//
// The document arrives in passes: the Qobuz page first, then the Magazine
// stories, then MusicBrainz Origin -> Relationships -> Discovery (see
// artist_qt.rs). Each MB section renders its own "Loading…" line and, when
// MusicBrainz is off in Settings or the artist has no confident MB match, is
// simply ABSENT — never an error frame, and nothing is requested.
//
// POC-NOTEs: blacklist banner, artist Scene, Share, Create Collection, radio
// engines (dropdown inert), multi-select, the sticky behavior of the JUMP TO
// bar (it scrolls with the page).

import QtQuick
import QtQuick.Controls
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

    readonly property var artist: JSON.parse(QbzArtist.artistJson)
    readonly property var topTracks: artist.topTracks || []
    readonly property var appearsOn: artist.appearsOn || []
    readonly property var releaseSections: artist.releaseSections || []
    readonly property var labels: artist.labels || []
    readonly property var similarArtists: artist.similarArtists || []
    readonly property var playlists: artist.playlists || []
    // MusicBrainz-driven sidebar payload (artist_qt.rs ArtistNetwork). Absent
    // on the very first frame of a cold document — every read below is
    // defaulted so a missing member can never throw.
    readonly property var network: artist.network || ({})
    readonly property var mbOrigin: network.origin || ({})
    readonly property var mbRelationships: network.relationships || ({})
    readonly property var stories: artist.stories || []

    property var coverMap: ({})
    property string activeJumpTab: "popular-tracks"
    property string artistTab: "catalog"
    property bool topTracksExpanded: false
    property bool appearsOnExpanded: false
    property bool otherExpanded: false
    property bool networkOpen: false
    property string netTab: "network"
    readonly property int preview: 5
    // Sidebar lists are unbounded upstream (an orchestra can list 150
    // members): show a slice, expand on demand — the delegates for the rest
    // are never instantiated.
    readonly property int sidebarPreview: 12
    property bool membersExpanded: false
    property bool groupsExpanded: false
    property bool collabsExpanded: false
    // Thumbs-downed discovery rows, by mbid. Session-only: the Slint app
    // persists these in its `discovery_dismiss` store, which this POC does
    // not open, so the rejection lasts as long as the process — it is NOT
    // written anywhere and makes no claim to be.
    property var dismissedDiscovery: ({})
    // The artist the view state (tab choice, dismissals) belongs to. Compared
    // on every republish so a mid-load pass never resets the user's choices.
    property string loadedArtistId: ""

    // Optimistic heart/pin state. The document is republished several times
    // per page now (stories, then each MusicBrainz section), and every
    // republish re-parses `artist` — a toggle written straight onto the parsed
    // object would silently pop back. Overrides live here instead and win over
    // whatever the document says, until the artist changes.
    property var localToggles: ({})
    function toggleState(key, fallback) {
        return localToggles[key] !== undefined ? localToggles[key] : fallback === true
    }
    function setToggleState(key, value) {
        var m = localToggles
        m[key] = value
        localToggles = Object.assign({}, m)
    }

    readonly property var discoveryRows: {
        var out = []
        var rows = network.discovery || []
        for (var i = 0; i < rows.length; i++) {
            if (!dismissedDiscovery[rows[i].mbid]) out.push(rows[i])
        }
        return out
    }

    // JUMP TO tabs from the present sections (ArtistState.jump-tabs).
    readonly property var jumpTabs: {
        var tabs = []
        if ((artist.bio || "") !== "") tabs.push({ "id": "about", "label": QbzSession.tr("About", QbzSession.trRev) })
        if (topTracks.length > 0) tabs.push({ "id": "popular-tracks", "label": QbzSession.tr("Popular Tracks", QbzSession.trRev) })
        for (var i = 0; i < releaseSections.length; i++) {
            if (releaseSections[i].releaseType !== "other")
                tabs.push({ "id": releaseSections[i].releaseType, "label": releaseSections[i].title })
        }
        if (appearsOn.length > 0) tabs.push({ "id": "appears-on", "label": QbzSession.tr("Appears On", QbzSession.trRev) })
        return tabs
    }

    // Two blocks, not one: artwork is QbzLibrary's signal and the releases
    // pager is QbzArtist's. Retargeting a mixed block wholesale would
    // silently orphan the other half — QML resolves handlers lazily, so the
    // discography would just stop loading with nothing in the log.
    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }

    Connections {
        target: QbzArtist
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
    Component.onCompleted: {
        syncArtistState()
        dispatchCovers()
    }
    onArtistChanged: {
        syncArtistState()
        dispatchCovers()
    }
    // Cover dispatch keys off the raw document (artist.artUrl etc.), so
    // re-fire when the parsed value actually changes (same stale race).
    onTopTracksChanged: dispatchCovers()
    onArtistTabChanged: if (artistTab === "library") dispatchLibCovers()

    // The document is republished several times per page (stories, then each
    // MusicBrainz section). Reset per-artist view state ONLY when the id
    // actually changed, or an enrichment pass would yank the sidebar tab back
    // under the user mid-read.
    function syncArtistState() {
        var id = artist.id || ""
        if (id === loadedArtistId)
            return
        loadedArtistId = id
        // Slint opens a fresh artist on Network, or on Magazine when
        // MusicBrainz is off (an empty Network tab is worse than none).
        netTab = (artist.network && artist.network.mbAvailable) ? "network" : "magazine"
        dismissedDiscovery = ({})
        localToggles = ({})
        membersExpanded = false
        groupsExpanded = false
        collabsExpanded = false
        dispatchedCovers = ({})
    }

    function dispatchLibCovers() {
        var items = libraryTab.libItems || []
        var urls = []
        for (var i = 0; i < items.length; i++) if (items[i].imageUrl) urls.push(items[i].imageUrl)
        dispatchArtwork(urls)
    }

    // Already-requested artwork keys. With the progressive republish the
    // dispatch runs once per pass, so re-sending the whole (potentially
    // several-hundred-entry) URL list every time is pure waste — send only
    // what is new.
    property var dispatchedCovers: ({})
    function dispatchArtwork(urls) {
        var fresh = []
        for (var i = 0; i < urls.length; i++) {
            var u = urls[i]
            if (!u || dispatchedCovers[u]) continue
            dispatchedCovers[u] = true
            fresh.push(u)
        }
        if (fresh.length > 0) QbzShell.sidebarArtworkWindow(JSON.stringify(fresh))
    }

    function dispatchCovers() {
        var urls = []
        if (artist.artUrl) urls.push(artist.artUrl)
        var i, j
        for (i = 0; i < topTracks.length; i++) if (topTracks[i].artUrl) urls.push(topTracks[i].artUrl)
        for (i = 0; i < releaseSections.length; i++)
            for (j = 0; j < releaseSections[i].cards.length; j++)
                if (releaseSections[i].cards[j].artUrl) urls.push(releaseSections[i].cards[j].artUrl)
        // Magazine story thumbnails ride the same pipeline (arc-cdn URLs).
        for (i = 0; i < stories.length; i++) if (stories[i].artUrl) urls.push(stories[i].artUrl)
        dispatchArtwork(urls)
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



    // Popular Tracks row (TrackRow with artwork + album column).
    component PopularTrackRow: Rectangle {
        property var row: ({})
        property int rowIndex: 0
        property bool showAlbum: true

        readonly property bool isActive: QbzPlayer.npTrackId !== "" && QbzPlayer.npTrackId === row.id
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
                    name: isActive && QbzPlayer.npPlaying ? "pause" : "play-fill"
                    width: 16
                    height: 16
                    tintName: "primary"
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzPlayer.playArtistTrack(row.id)
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
                    onClicked: QbzAlbum.openAlbum(row.albumId)
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
            // Favorite (live). Reads through the override map so the state
            // survives a document republish (see root.localToggles).
            Rectangle {
                property bool favorite: root.toggleState("track:" + row.id, row.isFavorite)
                width: 28
                height: 28
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: parent.favorite ? "heart-filled" : "heart"
                    width: 16
                    height: 16
                    tintName: parent.favorite ? "favorite" : (favArea.containsMouse ? "primary" : "muted")
                }
                MouseArea {
                    id: favArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.setToggleState("track:" + row.id, !parent.favorite)
                        QbzLibrary.libraryToggleFavorite("track", row.id)
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
            onDoubleClicked: QbzPlayer.playArtistTrack(row.id)
            onClicked: mouse.accepted = false
        }
    }

    // Sidebar link row (SidebarLink). `navigable` false = informational row:
    // no pointer cursor, no hover promise it cannot keep (used by the MB
    // Relationships rows, which have no destination in this port).
    component SidebarLink: Rectangle {
        property string label: ""
        property string iconName: "user"
        property string tooltip: ""
        property bool navigable: true
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
            cursorShape: navigable ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (navigable) parent.clicked()
            ToolTip.visible: containsMouse && tooltip !== ""
            ToolTip.text: tooltip
            ToolTip.delay: 400
        }
    }

    // Sidebar section heading (11px muted caps, letter-spaced).
    component SidebarSectionHeading: Text {
        color: theme.textMuted
        font.pixelSize: 11
        font.weight: theme.weightSemibold
        font.letterSpacing: 0.5
    }

    // Small 11px muted line — sub-group labels and the "Loading…"/empty
    // states inside the sidebar sections.
    component SidebarNote: Text {
        color: theme.textMuted
        font.pixelSize: 12
    }

    // One "KEY   value" row of the MB Origin block.
    component OriginRow: Item {
        property string key: ""
        property string value: ""
        // The host section Column carries 14px of left+right padding, and a
        // QML Positioner does NOT shrink its children for it — a right-aligned
        // value bound to the bare parent.width would run past the sidebar
        // edge. Subtract it here (this row only ever lives in that section).
        width: parent ? parent.width - 28 : 0
        height: 20
        Text {
            id: originKey
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: key
            color: theme.textMuted
            font.pixelSize: 11
            font.weight: theme.weightSemibold
            font.letterSpacing: 0.5
        }
        Text {
            anchors.right: parent.right
            anchors.left: originKey.right
            anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            text: value
            color: theme.textPrimary
            font.pixelSize: 13
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
        }
    }

    // One MB relationship sub-group (Members & Former / Member Of /
    // Collaborators) with a preview cap + expander.
    component RelationshipGroup: Column {
        id: relGroup
        property string title: ""
        property var rows: []
        property string iconName: "user"
        /// The MusicBrainz role this group represents ("member", "producer",
        /// …) — passed to the resolver so a same-name match in another role is
        /// not treated as this musician.
        property string roleKey: ""
        property bool expanded: false
        signal toggled()
        visible: rows.length > 0
        // -28 = the host section Column's left+right padding (see OriginRow).
        width: parent ? parent.width - 28 : 0
        spacing: 2
        topPadding: 2
        SidebarNote {
            text: relGroup.title
            font.pixelSize: 11
        }
        Repeater {
            model: relGroup.rows.length > root.sidebarPreview && !relGroup.expanded
                   ? relGroup.rows.slice(0, root.sidebarPreview)
                   : relGroup.rows
            delegate: SidebarLink {
                required property var modelData
                label: modelData.name
                tooltip: modelData.tooltip
                iconName: relGroup.iconName
                // Relationship rows carry a NAME, not a catalog id, so the
                // click resolves through MusicBrainz first. Only a confirmed
                // match navigates (resolve_musician logs and stays put
                // otherwise) — landing the user on a same-name artist is worse
                // than the row doing nothing.
                navigable: true
                onClicked: QbzArtist.resolveMusician(modelData.name, relGroup.roleKey || "")
            }
        }
        Text {
            visible: relGroup.rows.length > root.sidebarPreview
            leftPadding: 8
            // Same msgid pair the page's other expanders use — no new
            // catalog entries (all 8 locales already carry these).
            text: relGroup.expanded
                  ? QbzSession.tr("View less", QbzSession.trRev)
                  : QbzSession.tr("Load more", QbzSession.trRev)
            color: relMoreArea.containsMouse ? theme.textPrimary : theme.textSecondary
            font.pixelSize: 12
            MouseArea {
                id: relMoreArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: relGroup.toggled()
            }
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
                text: QbzSession.tr("See discography", QbzSession.trRev)
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
                        text: QbzSession.tr("Newest", QbzSession.trRev)
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
                    text: QbzSession.tr("Load more", QbzSession.trRev)
                    color: loadMoreArea.containsMouse ? theme.textPrimary : theme.textSecondary
                    font.pixelSize: 13
                }
                MouseArea {
                    id: loadMoreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzArtist.loadReleaseSection(artist.id, section.releaseType, section.cards.length)
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
                        text: QbzSession.tr("Read more", QbzSession.trRev)
                        color: readMoreArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: theme.fontLegal
                        MouseArea {
                            id: readMoreArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                var shell = root.parent
                                while (shell && shell.openTextModal === undefined) shell = shell.parent
                                if (!shell) return
                                // The Slint modal renders the attribution
                                // ("Source: TiVo") as a small line under the
                                // body; the shared text modal has one body
                                // slot, so it rides at the end.
                                var body = artist.bio || ""
                                if ((artist.bioSource || "") !== "")
                                    body += "\n\n" + QbzSession.tr("Source", QbzSession.trRev) + ": " + artist.bioSource
                                shell.openTextModal(artist.name || "", body)
                            }
                        }
                    }

                    Item { width: 1; height: 18 }
                    // Action row.
                    Row {
                        width: parent.width
                        spacing: 12
                        QbzCircleAction {
                            readonly property bool following: root.toggleState("artist", artist.isFollowing)
                            name: following ? "heart-filled" : "heart"
                            active: following
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: {
                                root.setToggleState("artist", !following)
                                QbzLibrary.libraryToggleFavorite("artist", artist.id)
                            }
                        }
                        QbzCircleAction {
                            id: radioBtn
                            name: "radio"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: radioPopup.openBelowRight(radioBtn)
                        }
                        QbzCircleAction {
                            name: "element-connect"
                            active: root.networkOpen
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: root.networkOpen = !root.networkOpen
                        }
                        QbzCircleAction {
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
                                        { "id": "catalog", "label": QbzSession.tr("From catalog", QbzSession.trRev), "count": 0 },
                                        { "id": "library", "label": QbzSession.tr("In library", QbzSession.trRev), "count": artist.libraryCount || 0 },
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
                visible: QbzArtist.artistLoading && topTracks.length === 0
                width: parent.width - 64
                height: 280
                Column {
                    anchors.centerIn: parent
                    spacing: 18
                    QbzSpinner { size: 36; anchors.horizontalCenter: parent.horizontalCenter }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("Loading artist…", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 13
                    }
                }
            }

            // ================= Catalog tab ================================
            Column {
                id: sectionAnchors
                visible: root.artistTab === "catalog" && !QbzArtist.artistLoading
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
                        text: QbzSession.tr("Popular Tracks", QbzSession.trRev)
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
                            onClicked: QbzPlayer.playArtistTop(false)
                        }
                    }
                    QbzCircleAction {
                        name: "square-check-big"
                        anchors.verticalCenter: parent.verticalCenter
                        // POC-NOTE: multi-select out of scope.
                    }
                    QbzCircleAction {
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
                        text: root.topTracksExpanded ? QbzSession.tr("View less", QbzSession.trRev) : QbzSession.tr("Load more", QbzSession.trRev)
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
                        text: QbzSession.tr("Latest release", QbzSession.trRev)
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
                        text: QbzSession.tr("Appears On", QbzSession.trRev)
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
                            text: root.appearsOnExpanded ? QbzSession.tr("View less", QbzSession.trRev) : QbzSession.tr("Load more", QbzSession.trRev)
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
                        text: QbzSession.tr("Playlists", QbzSession.trRev)
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
                                text: root.otherExpanded ? QbzSession.tr("Hide", QbzSession.trRev) : QbzSession.tr("Show", QbzSession.trRev)
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
                visible: root.artistTab === "library" && !QbzArtist.artistLoading
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
                    text: QbzSession.tr("Tracks", QbzSession.trRev)
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
                    text: QbzSession.tr("Albums", QbzSession.trRev)
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
        return JSON.parse(QbzLibrary.libraryJson)
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
                            { "id": "network", "label": QbzSession.tr("Network", QbzSession.trRev) },
                            { "id": "magazine", "label": QbzSession.tr("Magazine", QbzSession.trRev) },
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

                    // ORIGIN (MusicBrainz). Gated exactly like the Slint
                    // block: MB available AND (still loading OR the artist
                    // actually carries a life span / location). With MB off
                    // the whole block is absent — nothing was requested.
                    Column {
                        visible: network.mbAvailable === true
                                 && (network.originLoading === true || mbOrigin.hasData === true)
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 12
                        spacing: 6
                        SidebarNote {
                            visible: network.originLoading === true
                            text: QbzSession.tr("Loading origin…", QbzSession.trRev)
                        }
                        OriginRow {
                            visible: network.originLoading !== true && (mbOrigin.beginDate || "") !== ""
                            key: mbOrigin.isPerson ? QbzSession.tr("BORN", QbzSession.trRev)
                                                   : QbzSession.tr("FOUNDED", QbzSession.trRev)
                            value: mbOrigin.beginDate || ""
                        }
                        OriginRow {
                            visible: network.originLoading !== true && (mbOrigin.locationDisplay || "") !== ""
                            key: mbOrigin.isPerson ? QbzSession.tr("BORN IN", QbzSession.trRev)
                                                   : QbzSession.tr("FOUNDED IN", QbzSession.trRev)
                            // POC-NOTE: the Slint row is clickable and opens
                            // ArtistsByLocationView; that view has no port
                            // here, so the affordance is left out rather than
                            // rendered dead.
                            value: mbOrigin.locationDisplay || ""
                        }
                        OriginRow {
                            visible: network.originLoading !== true && (mbOrigin.endDate || "") !== ""
                            key: mbOrigin.isPerson ? QbzSession.tr("DIED", QbzSession.trRev)
                                                   : QbzSession.tr("DISBANDED", QbzSession.trRev)
                            value: mbOrigin.endDate || ""
                        }
                    }

                    // LABELS.
                    Column {
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 6
                        spacing: 4
                        SidebarSectionHeading { text: QbzSession.tr("LABELS", QbzSession.trRev) }
                        SidebarNote {
                            visible: labels.length === 0
                            text: QbzSession.tr("No label info", QbzSession.trRev)
                        }
                        Repeater {
                            model: labels
                            delegate: SidebarLink {
                                label: modelData.name
                                tooltip: modelData.name
                                iconName: "disc"
                                // POC-NOTE: no label view yet.
                            }
                        }
                    }
                    // SIMILAR ARTISTS.
                    Column {
                        visible: similarArtists.length > 0 || QbzArtist.artistLoading
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 6
                        spacing: 4
                        SidebarSectionHeading { text: QbzSession.tr("SIMILAR ARTISTS", QbzSession.trRev) }
                        SidebarNote {
                            visible: similarArtists.length === 0 && QbzArtist.artistLoading
                            text: QbzSession.tr("Loading…", QbzSession.trRev)
                        }
                        Repeater {
                            model: similarArtists
                            delegate: SidebarLink {
                                label: modelData.name
                                tooltip: modelData.name
                                iconName: "user"
                                onClicked: QbzArtist.openArtist(modelData.id)
                            }
                        }
                    }

                    // RELATIONSHIPS (MusicBrainz) — band members, the groups
                    // this artist belongs to, and studio collaborators.
                    Column {
                        visible: network.mbAvailable === true
                                 && (network.relationshipsLoading === true
                                     || mbRelationships.hasData === true)
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 6
                        spacing: 6
                        SidebarSectionHeading { text: QbzSession.tr("RELATIONSHIPS", QbzSession.trRev) }
                        SidebarNote {
                            visible: network.relationshipsLoading === true
                            text: QbzSession.tr("Loading…", QbzSession.trRev)
                        }
                        RelationshipGroup {
                            visible: network.relationshipsLoading !== true
                                     && (mbRelationships.members || []).length > 0
                            title: QbzSession.tr("Members & Former", QbzSession.trRev)
                            rows: mbRelationships.members || []
                            roleKey: "member"
                            iconName: "user"
                            expanded: root.membersExpanded
                            onToggled: root.membersExpanded = !root.membersExpanded
                        }
                        RelationshipGroup {
                            visible: network.relationshipsLoading !== true
                                     && (mbRelationships.groups || []).length > 0
                            title: QbzSession.tr("Member Of", QbzSession.trRev)
                            rows: mbRelationships.groups || []
                            roleKey: "member of"
                            iconName: "music"
                            expanded: root.groupsExpanded
                            onToggled: root.groupsExpanded = !root.groupsExpanded
                        }
                        RelationshipGroup {
                            visible: network.relationshipsLoading !== true
                                     && (mbRelationships.collaborators || []).length > 0
                            title: QbzSession.tr("Collaborators", QbzSession.trRev)
                            rows: mbRelationships.collaborators || []
                            roleKey: "collaborator"
                            iconName: "user"
                            expanded: root.collabsExpanded
                            onToggled: root.collabsExpanded = !root.collabsExpanded
                        }
                    }

                    // YOU MAY ALSO LIKE (MusicBrainz tag discovery, validated
                    // against Qobuz by the core). Rows without a resolved
                    // Qobuz id stay informational instead of dead-clicking.
                    Column {
                        visible: network.mbAvailable === true
                                 && (network.discoveryLoading === true || root.discoveryRows.length > 0)
                        width: parent.width
                        leftPadding: 14
                        rightPadding: 14
                        topPadding: 12
                        bottomPadding: 12
                        spacing: 4
                        SidebarSectionHeading { text: QbzSession.tr("YOU MAY ALSO LIKE", QbzSession.trRev) }
                        SidebarNote {
                            visible: network.discoveryLoading === true && root.discoveryRows.length === 0
                            text: QbzSession.tr("Loading…", QbzSession.trRev)
                        }
                        Repeater {
                            model: root.discoveryRows
                            delegate: Item {
                                required property var modelData
                                // -28 = the section Column's left+right
                                // padding (see OriginRow).
                                width: parent ? parent.width - 28 : 0
                                height: 28
                                SidebarLink {
                                    anchors.left: parent.left
                                    anchors.verticalCenter: parent.verticalCenter
                                    // Explicit width REPLACES the component's
                                    // own `parent.width` binding (leaving it
                                    // and anchoring both edges fights it).
                                    width: parent.width - 26
                                    label: modelData.name
                                    tooltip: modelData.name
                                    iconName: "user"
                                    navigable: modelData.qobuzId !== ""
                                    onClicked: QbzArtist.openArtist(modelData.qobuzId)
                                }
                                Rectangle {
                                    id: dismissBtn
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 24
                                    height: 24
                                    radius: 4
                                    color: dismissArea.containsMouse ? theme.surfaceElevated : "transparent"
                                    QbzIcon {
                                        anchors.centerIn: parent
                                        name: "thumbs-down"
                                        width: 12
                                        height: 12
                                        tintName: dismissArea.containsMouse ? "primary" : "muted"
                                    }
                                    MouseArea {
                                        id: dismissArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        // Session-only: drop the row now. The
                                        // Slint app also persists it under the
                                        // discovery tag; that store is not open
                                        // in this port (see the handoff report).
                                        onClicked: {
                                            var d = root.dismissedDiscovery
                                            d[modelData.mbid] = true
                                            root.dismissedDiscovery = Object.assign({}, d)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Magazine tab body — Qobuz editorial story teasers (limit 2,
            // like the official client). A story opens in the system browser.
            Flickable {
                visible: root.netTab === "magazine"
                width: parent.width
                height: parent.height - 45
                clip: true
                contentWidth: width
                contentHeight: magBody.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: magBody
                    width: parent.width
                    padding: 12
                    spacing: 10

                    SidebarNote {
                        visible: artist.storiesLoading === true && stories.length === 0
                        text: QbzSession.tr("Loading…", QbzSession.trRev)
                    }
                    SidebarNote {
                        visible: artist.storiesLoading !== true && stories.length === 0
                        text: QbzSession.tr("No stories for this artist", QbzSession.trRev)
                    }

                    Repeater {
                        model: stories
                        delegate: Rectangle {
                            required property var modelData
                            width: magBody.width - 24
                            height: storyCol.implicitHeight
                            radius: 8
                            color: storyArea.containsMouse ? theme.surfaceHover : "transparent"
                            Column {
                                id: storyCol
                                width: parent.width
                                padding: 6
                                spacing: 6
                                // 1:1 square thumbnail, height tracks width.
                                Rectangle {
                                    visible: (modelData.artUrl || "") !== ""
                                    width: storyCol.width - 12
                                    height: visible ? width : 0
                                    radius: 6
                                    color: theme.surfaceElevated
                                    clip: true
                                    RoundedImage {
                                        anchors.fill: parent
                                        source: root.coverMap[modelData.artUrl] || ""
                                        radius: 6
                                    }
                                }
                                Text {
                                    width: storyCol.width - 12
                                    text: modelData.title
                                    color: theme.textPrimary
                                    font.pixelSize: 13
                                    font.weight: theme.weightSemibold
                                    wrapMode: Text.WordWrap
                                }
                                Text {
                                    visible: (modelData.author || "") !== ""
                                    width: storyCol.width - 12
                                    text: modelData.author
                                    color: theme.textMuted
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                }
                            }
                            MouseArea {
                                id: storyArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: if ((modelData.url || "") !== "") Qt.openUrlExternally(modelData.url)
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Radio dropdown (INERT items — POC-NOTE: radio engines) ----------
    QbzContextMenu {
        id: radioPopup
        menuWidth: 180
            Repeater {
                model: [QbzSession.tr("QBZ Radio", QbzSession.trRev), QbzSession.tr("Qobuz Radio", QbzSession.trRev)]
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
                    { "label": QbzSession.tr("Create Artist Collection", QbzSession.trRev), "icon": "library-big", "action": "stub" },
                    { "label": QbzSession.tr("Artist Scene", QbzSession.trRev), "icon": "map-pin", "action": "stub" },
                    { "label": QbzSession.tr("Share", QbzSession.trRev), "icon": "link", "action": "stub" },
                    { "label": root.toggleState("artistPin", artist.isPinned) ? QbzSession.tr("Unpin", QbzSession.trRev) : QbzSession.tr("Pin", QbzSession.trRev), "icon": root.toggleState("artistPin", artist.isPinned) ? "pin-filled" : "pin", "action": "pin" },
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
                                root.setToggleState("artistPin", !root.toggleState("artistPin", artist.isPinned))
                                QbzLibrary.togglePin("artist", artist.id, artist.name, "", artist.artUrl)
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
                        text: QbzSession.tr("Blacklist artist", QbzSession.trRev)
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
                    { "label": QbzSession.tr("Play all next", QbzSession.trRev), "icon": "list-start", "action": "next-all" },
                    { "label": QbzSession.tr("Add all to queue", QbzSession.trRev), "icon": "list-end", "action": "queue-all" },
                    { "label": QbzSession.tr("Shuffle all", QbzSession.trRev), "icon": "shuffle", "action": "shuffle-all" },
                    { "label": QbzSession.tr("Add all to playlist", QbzSession.trRev), "icon": "list-music", "action": "playlist-all" },
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
                            if (a === "shuffle-all") QbzPlayer.playArtistTop(true)
                            else if (a === "next-all") QbzPlayer.playArtistTop(false)
                            else if (a === "queue-all") QbzPlayer.enqueueArtistTop()
                            // playlist-all: inert (no picker) — POC-NOTE.
                        }
                    }
                }
            }
        }
}
