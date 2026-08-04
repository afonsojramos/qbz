// KioskSearch — the kiosk Search results page (shell/KioskSearch.slint, 579
// lines). Reads the SAME search document the desktop SearchView reads
// (albums / tracks / artists / artistsCarousel / playlists + the tab int),
// rendered with the four lightweight kiosk primitives.
//
// The QUERY is not here: it is typed in the KioskShell back bar
// (SearchActions.submit -> QbzSearch.searchSubmit), and this view is only the
// tabbed results. There is no search field, no filter row, no "Load more" and
// no skeleton/empty/loading copy — KioskSearch.slint has none of them, so a
// query with no results renders the 48px tab strip and a blank pane
// (KioskSearch.slint:181-411: every section is gated on `length > 0` and the
// hero on `kind != ""`).
//
// NAV (KioskSearch.slint:97-144): the index space is the 5 tabs followed by
// the ACTIVE tab's items as a uniform grid. The All dashboard publishes
// TABS ONLY — its hero/preview mosaic is not a uniform grid, so keyboard nav
// on All reaches the 5 tab entries and nothing else. That is deliberate.
//
// Slint's `viewport-y` is negative as you scroll down; Qt's `contentY` is its
// positive twin — every scroll-into-view write below is already translated,
// and each is made from a function deferred with Qt.callLater, never from a
// binding that feeds layout.
//
// ARTWORK: search rows carry their own Rust-resolved local path (`artPath`,
// search_qt.rs CardRow/TrackRow/ArtistRow/PlaylistRow), so this view needs no
// artwork-window plumbing. The kiosk primitives read the resolved path under
// `artwork`, so the row objects are mapped once per document (below) rather
// than per delegate.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    // KioskSearch.slint:90 — no `background:` is set on the exported
    // component, i.e. transparent over the shell's own surface.
    color: "transparent"

    // KioskSearch.slint:95.
    property real pad: 16

    QbzTheme { id: theme }

    // ---------------------------------------------------------------------
    // Document (search_qt.rs SearchPageDoc :1774-1796 / MostPopularDoc :1764-1771)
    // ---------------------------------------------------------------------
    readonly property var doc: root.parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzSearch.searchJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var albums: root.doc.albums || []
    readonly property var tracks: root.doc.tracks || []
    readonly property var artists: root.doc.artists || []
    readonly property var artistsCarousel: root.doc.artistsCarousel || []
    readonly property var playlists: root.doc.playlists || []
    readonly property var mp: root.doc.mostPopular || ({})
    readonly property int tab: root.doc.tab || 0
    readonly property int tracksTotal: root.doc.tracksTotal || 0

    // ---------------------------------------------------------------------
    // Row -> primitive mapping. Done ONCE per document: the delegates below
    // are handed a finished object, so nothing re-allocates while scrolling.
    // ---------------------------------------------------------------------
    function cardOf(r) {
        return {
            "id": r.id || "",
            "title": r.title || "",
            "artist": r.artist || "",
            "artwork": r.artPath || "",
            "qualityTier": r.qualityTier || ""
        }
    }
    function trackOf(r) {
        return {
            "id": r.id || "",
            "title": r.title || "",
            "artist": r.artist || "",
            "duration": r.duration || "",
            "artwork": r.artPath || ""
        }
    }
    function artistOf(r) {
        // ArtistRow's display name is `title`, NOT `name` (search_qt.rs:214).
        return {
            "id": r.id || "",
            "title": r.title || "",
            "artwork": r.artPath || ""
        }
    }
    function playlistOf(r) {
        return {
            "id": r.id || "",
            "title": r.title || "",
            "artwork": r.artPath || ""
        }
    }
    function mapRows(rows, fn) {
        var out = []
        for (var i = 0; i < rows.length; i++)
            out.push(fn(rows[i]))
        return out
    }

    readonly property var albumCards: root.mapRows(root.albums, root.cardOf)
    readonly property var trackRows: root.mapRows(root.tracks, root.trackOf)
    readonly property var artistCards: root.mapRows(root.artists, root.artistOf)
    readonly property var artistCarouselCards: root.mapRows(root.artistsCarousel, root.artistOf)
    readonly property var playlistCards: root.mapRows(root.playlists, root.playlistOf)

    readonly property string heroKind: root.mp.kind || ""
    readonly property var heroAlbum: root.mp.album ? root.cardOf(root.mp.album) : ({})
    readonly property var heroArtist: root.mp.artist ? root.artistOf(root.mp.artist) : ({})
    readonly property var heroTrack: root.mp.track ? root.trackOf(root.mp.track) : ({})
    readonly property string heroQualityLabel: root.mp.qualityLabel || ""

    // ---------------------------------------------------------------------
    // Nav geometry (KioskSearch.slint:103-135). The clamp Slint writes by hand
    // at :122 lives inside the model's own publish (kiosk_nav_qt.rs:266-274).
    // ---------------------------------------------------------------------
    function publishNav() {
        var columns = 1
        var count = 5
        if (root.tab === 1) {
            columns = 6
            count = 5 + root.albums.length
        } else if (root.tab === 2) {
            columns = 1
            count = 5 + root.tracks.length
        } else if (root.tab === 3) {
            columns = 7
            count = 5 + root.artists.length
        } else if (root.tab === 4) {
            columns = 6
            count = 5 + root.playlists.length
        }
        QbzKioskNav.publishNav(5, columns, count, false)
    }

    Component.onCompleted: root.publishNav()
    onTabChanged: root.publishNav()

    readonly property int navLenProbe: root.albums.length + root.tracks.length
        + root.artists.length + root.playlists.length
    onNavLenProbeChanged: root.publishNav()

    readonly property bool contentFocus: QbzKioskNav.navActive && QbzKioskNav.zone === "content"

    // Enter on a tab entry — the same switch the tab taps drive
    // (KioskSearch.slint:137-144). The per-tab containers below answer the
    // same pulse for `index >= tabs`, so the two never overlap.
    Connections {
        target: QbzKioskNav

        function onActivateSeqChanged() {
            if (root.contentFocus && QbzKioskNav.index < QbzKioskNav.tabs)
                QbzSearch.searchTabChanged(QbzKioskNav.index)
        }
    }

    // ---------------------------------------------------------------------
    // Local components (SearchTab KioskSearch.slint:17-51,
    // SectionHeader :55-88)
    // ---------------------------------------------------------------------
    component SearchTab: Rectangle {
        id: st

        property string label: ""
        property int tabIndex: 0
        property bool navFocused: false
        readonly property bool active: root.tab === st.tabIndex

        width: stLabel.implicitWidth + 4
        height: parent ? parent.height : 0
        color: "transparent"
        border.width: st.navFocused ? 2 : 0
        border.color: theme.accent
        radius: theme.radiusSm

        Text {
            id: stLabel
            anchors.left: st.left
            anchors.leftMargin: 2
            anchors.top: st.top
            anchors.bottom: stRule.top
            text: st.label
            color: st.active ? theme.textPrimary : theme.textMuted
            font.pixelSize: 15
            font.weight: theme.weightSemibold
            verticalAlignment: Text.AlignVCenter
        }

        Rectangle {
            id: stRule
            anchors.left: st.left
            anchors.leftMargin: 2
            anchors.right: st.right
            anchors.rightMargin: 2
            anchors.bottom: st.bottom
            height: 2
            radius: 2
            color: st.active ? theme.accent : "transparent"
        }

        MouseArea {
            anchors.fill: st
            onClicked: QbzSearch.searchTabChanged(st.tabIndex)
        }
    }

    component SectionHeader: Item {
        id: sh

        property string title: ""
        property bool showAll: true
        signal viewAll()

        height: 34

        Text {
            anchors.left: sh.left
            anchors.verticalCenter: sh.verticalCenter
            text: sh.title
            color: theme.textPrimary
            font.pixelSize: 18
            font.weight: theme.weightBold
        }

        Item {
            id: shAll
            visible: sh.showAll
            anchors.right: sh.right
            anchors.top: sh.top
            anchors.bottom: sh.bottom
            width: 74

            Text {
                anchors.fill: shAll
                text: QbzSession.tr("View all", QbzSession.trRev)
                color: theme.accent
                font.pixelSize: 13
                font.weight: theme.weightSemibold
                horizontalAlignment: Text.AlignRight
                verticalAlignment: Text.AlignVCenter
            }

            MouseArea {
                anchors.fill: shAll
                onClicked: sh.viewAll()
            }
        }
    }

    // ---------------------------------------------------------------------
    // Tab strip (KioskSearch.slint:150-169). The 1px rule sits on the strip's
    // BOTTOM edge (`y: parent.height - 1px`, :165), separating tabs from
    // content — not a top border.
    // ---------------------------------------------------------------------
    Rectangle {
        id: tabStrip
        anchors.top: root.top
        anchors.left: root.left
        anchors.right: root.right
        height: 48
        color: theme.surfaceMain

        Row {
            id: tabsRow
            x: root.pad
            anchors.top: tabStrip.top
            anchors.bottom: tabStrip.bottom
            spacing: 22

            SearchTab {
                label: QbzSession.tr("All", QbzSession.trRev)
                tabIndex: 0
                navFocused: root.contentFocus && QbzKioskNav.index === 0
            }
            SearchTab {
                label: QbzSession.tr("Albums", QbzSession.trRev)
                tabIndex: 1
                navFocused: root.contentFocus && QbzKioskNav.index === 1
            }
            SearchTab {
                label: QbzSession.tr("Tracks", QbzSession.trRev)
                tabIndex: 2
                navFocused: root.contentFocus && QbzKioskNav.index === 2
            }
            SearchTab {
                label: QbzSession.tr("Artists", QbzSession.trRev)
                tabIndex: 3
                navFocused: root.contentFocus && QbzKioskNav.index === 3
            }
            SearchTab {
                label: QbzSession.tr("Playlists", QbzSession.trRev)
                tabIndex: 4
                navFocused: root.contentFocus && QbzKioskNav.index === 4
            }
        }

        Rectangle {
            y: tabStrip.height - 1
            width: tabStrip.width
            height: 1
            color: theme.borderSubtle
        }
    }

    // ---------------------------------------------------------------------
    // Content (KioskSearch.slint:172-577). One arm mounted at a time, exactly
    // as Slint's `if` chain does.
    // ---------------------------------------------------------------------
    Item {
        id: content
        anchors.top: tabStrip.bottom
        anchors.left: root.left
        anchors.right: root.right
        anchors.bottom: root.bottom
        clip: true

        Loader {
            anchors.fill: content
            active: root.tab === 0
            sourceComponent: allDashboard
        }
        Loader {
            anchors.fill: content
            active: root.tab === 1
            sourceComponent: albumsTab
        }
        Loader {
            anchors.fill: content
            active: root.tab === 2
            sourceComponent: tracksTab
        }
        Loader {
            anchors.fill: content
            active: root.tab === 3
            sourceComponent: artistsTab
        }
        Loader {
            anchors.fill: content
            active: root.tab === 4
            sourceComponent: playlistsTab
        }
    }

    // ---- All — the dashboard (KioskSearch.slint:181-411) -----------------
    Component {
        id: allDashboard

        Flickable {
            id: allFlick
            contentWidth: allFlick.width
            contentHeight: dash.height + 2 * root.pad
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: dash
                x: root.pad
                y: root.pad
                width: allFlick.width - 2 * root.pad
                spacing: 26

                // ---- Most-popular hero (:190-280), polymorphic ----------
                Column {
                    visible: root.heroKind !== ""
                    width: dash.width
                    spacing: 10

                    Row {
                        spacing: 8

                        QbzIcon {
                            id: crownGlyph
                            name: "award"
                            width: 18
                            height: 18
                            tintName: "warning"
                        }

                        Text {
                            anchors.verticalCenter: crownGlyph.verticalCenter
                            text: QbzSession.tr("Most popular", QbzSession.trRev)
                            color: theme.textSecondary
                            font.pixelSize: 14
                            font.weight: theme.weightBold
                        }
                    }

                    // The hero carries no nav ring: the All tab publishes no
                    // items (:118-121).
                    Loader {
                        active: root.heroKind === "artist"
                        sourceComponent: KioskArtistCard {
                            artSize: 150
                            artist: root.heroArtist
                            onClicked: function (id) {
                                QbzArtist.openArtist(id)
                            }
                        }
                    }

                    Loader {
                        active: root.heroKind === "album"
                        sourceComponent: KioskCard {
                            artSize: 150
                            album: root.heroAlbum
                            onClicked: function (id) {
                                QbzAlbum.openAlbum(id)
                            }
                        }
                    }

                    // Track hero — a bespoke 380x96 card, NOT KioskTrackRow
                    // (:227-278).
                    Loader {
                        active: root.heroKind === "track"
                        sourceComponent: Rectangle {
                            id: heroTrackCard
                            width: 380
                            height: 96
                            radius: theme.radiusMd
                            color: theme.surfaceCard

                            Row {
                                x: 8
                                y: 8
                                width: heroTrackCard.width - 16
                                height: heroTrackCard.height - 16
                                spacing: 12

                                Rectangle {
                                    id: heroArt
                                    width: 80
                                    height: 80
                                    radius: theme.radiusSm
                                    color: theme.surfaceElevated
                                    clip: true

                                    // Mounted only when there is a resolved
                                    // cover (:241) — otherwise the bare
                                    // elevated square shows.
                                    Loader {
                                        anchors.fill: heroArt
                                        active: (root.heroTrack.artwork || "") !== ""
                                        sourceComponent: RoundedImage {
                                            source: root.heroTrack.artwork || ""
                                            radius: theme.radiusSm
                                            fit: "crop"
                                        }
                                    }
                                }

                                Column {
                                    id: heroText
                                    width: heroTrackCard.width - 16 - heroArt.width - 12
                                    anchors.verticalCenter: heroArt.verticalCenter
                                    spacing: 3

                                    Text {
                                        width: heroText.width
                                        text: root.heroTrack.title || ""
                                        color: theme.textPrimary
                                        font.pixelSize: 16
                                        font.weight: theme.weightBold
                                        elide: Text.ElideRight
                                        wrapMode: Text.NoWrap
                                        maximumLineCount: 1
                                    }

                                    Text {
                                        width: heroText.width
                                        text: root.heroTrack.artist || ""
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        elide: Text.ElideRight
                                        wrapMode: Text.NoWrap
                                        maximumLineCount: 1
                                    }

                                    Text {
                                        visible: root.heroQualityLabel !== ""
                                        width: heroText.width
                                        text: root.heroQualityLabel
                                        color: theme.textMuted
                                        font.pixelSize: 11
                                    }
                                }
                            }

                            MouseArea {
                                anchors.fill: heroTrackCard
                                onClicked: QbzPlayer.playTrack(root.heroTrack.id || "")
                            }
                        }
                    }
                }

                // ---- Artists preview (:283-309), hero-deduped carousel ----
                Column {
                    visible: root.artistCarouselCards.length > 0
                    width: dash.width
                    spacing: 10

                    SectionHeader {
                        width: dash.width
                        title: QbzSession.tr("Artists", QbzSession.trRev)
                        onViewAll: QbzSearch.searchTabChanged(3)
                    }

                    Row {
                        spacing: 14

                        Repeater {
                            model: root.artistCarouselCards.slice(0, 6)

                            delegate: KioskArtistCard {
                                required property var modelData

                                artSize: 120
                                artist: modelData
                                onClicked: function (id) {
                                    QbzArtist.openArtist(id)
                                }
                            }
                        }
                    }
                }

                // ---- Albums preview (:312-336) ---------------------------
                Column {
                    visible: root.albumCards.length > 0
                    width: dash.width
                    spacing: 10

                    SectionHeader {
                        width: dash.width
                        title: QbzSession.tr("Albums", QbzSession.trRev)
                        onViewAll: QbzSearch.searchTabChanged(1)
                    }

                    Row {
                        spacing: 14

                        Repeater {
                            model: root.albumCards.slice(0, 6)

                            delegate: KioskCard {
                                required property var modelData

                                artSize: 140
                                album: modelData
                                onClicked: function (id) {
                                    QbzAlbum.openAlbum(id)
                                }
                            }
                        }
                    }
                }

                // ---- Tracks preview (:339-356) ---------------------------
                // The ONLY section with a conditional "View all" (:343).
                Column {
                    visible: root.trackRows.length > 0
                    width: dash.width
                    spacing: 6

                    SectionHeader {
                        width: dash.width
                        title: QbzSession.tr("Tracks", QbzSession.trRev)
                        showAll: root.tracksTotal > 6
                        onViewAll: QbzSearch.searchTabChanged(2)
                    }

                    Repeater {
                        model: root.trackRows.slice(0, 6)

                        delegate: KioskTrackRow {
                            required property var modelData

                            width: dash.width
                            track: modelData
                            onClicked: QbzPlayer.playTrack(modelData.id || "")
                        }
                    }
                }

                // ---- Playlists preview (:359-409) ------------------------
                Column {
                    visible: root.playlistCards.length > 0
                    width: dash.width
                    spacing: 10

                    SectionHeader {
                        width: dash.width
                        title: QbzSession.tr("Playlists", QbzSession.trRev)
                        onViewAll: QbzSearch.searchTabChanged(4)
                    }

                    Row {
                        spacing: 14

                        Repeater {
                            model: root.playlistCards.slice(0, 6)

                            // A bespoke 140px card, not KioskCard (:373-405).
                            delegate: Item {
                                id: plPrev

                                required property var modelData

                                width: 140
                                height: plPrevBody.height

                                Column {
                                    id: plPrevBody
                                    width: 140
                                    spacing: 7

                                    Rectangle {
                                        id: plPrevArt
                                        width: 140
                                        height: 140
                                        radius: theme.radiusSm
                                        color: theme.surfaceElevated
                                        clip: true

                                        RoundedImage {
                                            anchors.fill: plPrevArt
                                            source: plPrev.modelData.artwork || ""
                                            radius: theme.radiusSm
                                            fit: "crop"
                                        }
                                    }

                                    Text {
                                        width: 140
                                        text: plPrev.modelData.title || ""
                                        color: theme.textPrimary
                                        font.pixelSize: 13
                                        font.weight: theme.weightSemibold
                                        elide: Text.ElideRight
                                        wrapMode: Text.NoWrap
                                        maximumLineCount: 1
                                    }
                                }

                                MouseArea {
                                    anchors.fill: plPrev
                                    onClicked: QbzBridge.openPlaylist(plPrev.modelData.id || "")
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- Albums tab (KioskSearch.slint:414-419) --------------------------
    // Only `albums` is passed; columns/gap/pad keep the primitive's defaults,
    // and the grid owns its own ring, Enter pulse and scroll-into-view.
    Component {
        id: albumsTab

        KioskAlbumGrid {
            albums: root.albumCards
            onOpen: function (id) {
                QbzAlbum.openAlbum(id)
            }
        }
    }

    // ---- Tracks tab (KioskSearch.slint:422-453) --------------------------
    Component {
        id: tracksTab

        ListView {
            id: tracksList
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            spacing: 0
            model: root.trackRows

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: root.contentFocus
                && tracksList.focusedItem >= 0
                && tracksList.focusedItem < root.trackRows.length

            // 62px pitch = the KioskTrackRow height, no spacing.
            function scrollFocusIntoView() {
                if (!tracksList.itemFocused)
                    return
                var top = tracksList.focusedItem * 62
                if (top < tracksList.contentY)
                    tracksList.contentY = top - 8
                else if (top + 62 > tracksList.contentY + tracksList.height)
                    tracksList.contentY = top + 62 - tracksList.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(tracksList.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (tracksList.itemFocused)
                        QbzPlayer.playTrack(root.trackRows[tracksList.focusedItem].id || "")
                }
            }

            delegate: KioskTrackRow {
                id: trackRow

                required property var modelData
                required property int index

                width: tracksList.width
                track: trackRow.modelData
                navFocused: tracksList.itemFocused && tracksList.focusedItem === trackRow.index
                onClicked: QbzPlayer.playTrack(trackRow.modelData.id || "")
            }
        }
    }

    // ---- Artists tab (KioskSearch.slint:456-501) -------------------------
    Component {
        id: artistsTab

        Flickable {
            id: agrid
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int cols: 7
            readonly property real gap: 14
            readonly property real cell: (agrid.width - 2 * root.pad - (agrid.cols - 1) * agrid.gap) / agrid.cols
            // +26px is the name-label band under the round avatar.
            readonly property real band: agrid.cell + 26

            contentWidth: agrid.width
            contentHeight: Math.ceil(root.artistCards.length / agrid.cols) * (agrid.band + agrid.gap) + root.pad

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: root.contentFocus
                && agrid.focusedItem >= 0
                && agrid.focusedItem < root.artistCards.length
            readonly property real focusTop: root.pad
                + Math.floor(agrid.focusedItem / agrid.cols) * (agrid.band + agrid.gap)

            function scrollFocusIntoView() {
                if (!agrid.itemFocused)
                    return
                if (agrid.focusTop < agrid.contentY)
                    agrid.contentY = agrid.focusTop - 8
                else if (agrid.focusTop + agrid.band > agrid.contentY + agrid.height)
                    agrid.contentY = agrid.focusTop + agrid.band - agrid.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(agrid.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (agrid.itemFocused)
                        QbzArtist.openArtist(root.artistCards[agrid.focusedItem].id || "")
                }
            }

            Item {
                width: agrid.contentWidth
                height: agrid.contentHeight

                Repeater {
                    model: root.artistCards

                    delegate: KioskArtistCard {
                        id: artistCell

                        required property var modelData
                        required property int index

                        x: root.pad + (artistCell.index % agrid.cols) * (agrid.cell + agrid.gap)
                        y: root.pad + Math.floor(artistCell.index / agrid.cols) * (agrid.band + agrid.gap)
                        artSize: agrid.cell
                        artist: artistCell.modelData
                        navFocused: agrid.itemFocused && agrid.focusedItem === artistCell.index
                        onClicked: function (id) {
                            QbzArtist.openArtist(id)
                        }
                    }
                }
            }
        }
    }

    // ---- Playlists tab (KioskSearch.slint:504-576) -----------------------
    Component {
        id: playlistsTab

        Flickable {
            id: pgrid
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int cols: 6
            readonly property real gap: 16
            readonly property real cell: (pgrid.width - 2 * root.pad - (pgrid.cols - 1) * pgrid.gap) / pgrid.cols
            // +42px is the title band.
            readonly property real band: pgrid.cell + 42

            contentWidth: pgrid.width
            contentHeight: Math.ceil(root.playlistCards.length / pgrid.cols) * (pgrid.band + pgrid.gap) + root.pad

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: root.contentFocus
                && pgrid.focusedItem >= 0
                && pgrid.focusedItem < root.playlistCards.length
            readonly property real focusTop: root.pad
                + Math.floor(pgrid.focusedItem / pgrid.cols) * (pgrid.band + pgrid.gap)

            function scrollFocusIntoView() {
                if (!pgrid.itemFocused)
                    return
                if (pgrid.focusTop < pgrid.contentY)
                    pgrid.contentY = pgrid.focusTop - 8
                else if (pgrid.focusTop + pgrid.band > pgrid.contentY + pgrid.height)
                    pgrid.contentY = pgrid.focusTop + pgrid.band - pgrid.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(pgrid.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (pgrid.itemFocused)
                        QbzBridge.openPlaylist(root.playlistCards[pgrid.focusedItem].id || "")
                }
            }

            Item {
                width: pgrid.contentWidth
                height: pgrid.contentHeight

                Repeater {
                    model: root.playlistCards

                    // The ring is drawn by the cell itself — there is no shared
                    // primitive for playlists. Card ring family: 3px + a 0.12
                    // accent tint (:540-543).
                    delegate: Rectangle {
                        id: plCell

                        required property var modelData
                        required property int index

                        readonly property bool focused: pgrid.itemFocused
                            && pgrid.focusedItem === plCell.index

                        x: root.pad + (plCell.index % pgrid.cols) * (pgrid.cell + pgrid.gap)
                        y: root.pad + Math.floor(plCell.index / pgrid.cols) * (pgrid.band + pgrid.gap)
                        width: pgrid.cell
                        height: pgrid.band
                        border.width: plCell.focused ? 3 : 0
                        border.color: theme.accent
                        radius: theme.radiusSm
                        color: plCell.focused
                            ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.12)
                            : "transparent"

                        Column {
                            width: pgrid.cell
                            spacing: 7

                            Rectangle {
                                id: plArt
                                width: pgrid.cell
                                height: pgrid.cell
                                radius: theme.radiusSm
                                color: theme.surfaceElevated
                                clip: true

                                RoundedImage {
                                    anchors.fill: plArt
                                    source: plCell.modelData.artwork || ""
                                    radius: theme.radiusSm
                                    fit: "crop"
                                }
                            }

                            Text {
                                width: pgrid.cell
                                text: plCell.modelData.title || ""
                                color: theme.textPrimary
                                font.pixelSize: 14
                                font.weight: theme.weightSemibold
                                elide: Text.ElideRight
                                wrapMode: Text.NoWrap
                                maximumLineCount: 1
                            }
                        }

                        MouseArea {
                            anchors.fill: plCell
                            onClicked: QbzBridge.openPlaylist(plCell.modelData.id || "")
                        }
                    }
                }
            }
        }
    }
}
