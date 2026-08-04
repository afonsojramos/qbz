// KioskLibrary — lightweight kiosk Library (Favorites) view
// (shell/KioskLibrary.slint). The reference header's v1 limits, verbatim:
//
//   Reads the SAME FavoritesState the desktop FavoritesView reads, rendered
//   with the simple kiosk cards. Tabs Tracks/Albums/Artists/Playlists/Labels
//   switch via FavoritesActions.select-tab (Rust lazy-loads each tab). Covers
//   are push-only except album covers (windowed — v1 shows the first ~60; the
//   window hook is a follow-up).
//
//   Keyboard/gamepad nav: the nav index space is the 5 tabs (indices 0-4)
//   followed by the active tab's items as a uniform grid (tracks = a
//   1-column list). The view publishes that geometry into KioskNavState; each
//   container mirrors the focus index (ring + scroll-into-view) and answers
//   the Enter pulse. viewport-y is only ever WRITTEN from callbacks on a
//   settled layout, never read by layout-feeding bindings (the
//   AlbumCollectionView "Recursion detected" panic class).
//
// The "first ~60" cap is literal and is reproduced HERE: Slint's cap is the
// Rust-side FAV_ALBUMS_WINDOW = (0, 59) (qbz/src/favorites.rs:929) and the
// kiosk grid never fires the window-changed hook, so cards 61+ keep a bare
// surfaceElevated square forever. In Qt the cap is whatever this file passes
// to libraryArtworkWindow — hence `albumCoverCap` below, dispatched once per
// Albums-tab entry and never on scroll. Non-album tabs are push-only, i.e.
// every row's key is dispatched.
//
// Three differences from the Slint that come from the Qt data model, not from
// a design choice:
//   * Qt has ONE merged feed document (QbzLibrary.libraryJson) instead of five
//     lazily-loaded models, so the five lists are JS filters over it — the
//     same predicates views/LibraryView.qml:422-427 already uses. There is no
//     FavoritesActions.select-tab invokable, so `activeTab` is view-local and
//     no per-tab history entry is recorded.
//   * The Slint lists are the `-visible` (searched + sorted) models; this view
//     exposes NO search, sort or filter UI, so it takes the raw feed subsets in
//     feed order. There is no sort/filter chrome to add here.
//   * Covers are id-keyed through libraryArtworkReady into `artMap`. The kiosk
//     cards take the RESOLVED path inside their row object, so the tracks /
//     artists / labels rows are re-wrapped per DELEGATE (only mounted rows pay
//     for a cover arrival) and only the album array — whose consumer owns its
//     own Repeater — is rebuilt whole. The Playlists tab does not use this path
//     at all: its cover is covers[0], the first MEMBER-album cover, which is
//     what Slint's `cover1` (collage slot 0) holds; artMap[artKey] would be the
//     playlist's OWN graphic instead, and blank for every playlist that has none.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    color: "transparent"

    QbzTheme { id: theme }

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    readonly property real pad: 16

    // No FavoritesActions.select-tab in Qt: the tab is view state, and the
    // Slint default is "tracks" (state.slint:5133).
    property string activeTab: "tracks"

    readonly property var tabIds: ["tracks", "albums", "artists", "playlists", "labels"]

    function tabLabel(id) {
        return id === "tracks" ? root.t("Tracks")
            : id === "albums" ? root.t("Albums")
            : id === "artists" ? root.t("Artists")
            : id === "playlists" ? root.t("Playlists")
            : root.t("Labels")
    }

    // ---- the merged feed and its five subsets ---------------------------
    // ONE guarded parse per document: a raw JSON.parse inside a binding throws
    // on the pre-publish frame and takes the whole view down.
    readonly property var feed: root.parseFeed()
    function parseFeed() {
        try {
            var f = JSON.parse(QbzLibrary.libraryJson)
            return (f instanceof Array) ? f : []
        } catch (e) {
            return []
        }
    }

    // `group` empty = the kind alone decides (artists, labels).
    function subset(kind, group) {
        var out = []
        var f = root.feed
        for (var i = 0; i < f.length; i++) {
            var x = f[i]
            if (x.kind === kind && (group === "" || x.group === group))
                out.push(x)
        }
        return out
    }

    readonly property var trackRows: root.subset("track", "favorites")
    readonly property var albumRows: root.subset("album", "favorites")
    readonly property var artistRows: root.subset("artist", "")
    readonly property var playlistRows: root.subset("playlist", "favorites")
    readonly property var labelRows: root.subset("label", "")

    // ---- covers ----------------------------------------------------------
    // {artKey: file://path}. A NEW object per arrival — a same-reference
    // assignment is not a change in QML.
    property var artMap: ({})

    Connections {
        target: QbzLibrary

        function onLibraryArtworkReady(key, path) {
            var m = root.artMap
            m[key] = path
            root.artMap = Object.assign({}, m)
        }
    }

    // qbz/src/favorites.rs:929 — FAV_ALBUMS_WINDOW = (0, 59).
    readonly property int albumCoverCap: 60

    function dispatchArtwork() {
        var rows = root.activeTab === "tracks" ? root.trackRows
            : root.activeTab === "albums" ? root.albumRows
            : root.activeTab === "artists" ? root.artistRows
            : root.activeTab === "labels" ? root.labelRows
            : []
        var n = root.activeTab === "albums"
            ? Math.min(rows.length, root.albumCoverCap)
            : rows.length
        var keys = []
        for (var i = 0; i < n; i++)
            if (rows[i].artKey)
                keys.push(rows[i].artKey)
        if (keys.length > 0)
            QbzLibrary.libraryArtworkWindow(JSON.stringify(keys))
    }

    // The album array is the ONE consumer that owns its Repeater, so the
    // resolved path has to live in the row object.
    readonly property var albumCards: root.buildAlbumCards()
    function buildAlbumCards() {
        var rows = root.albumRows
        var out = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            out.push({
                "id": r.id,
                "title": r.title,
                "artist": r.artist,
                "qualityTier": r.qualityTier,
                // Past the cap no cover is ever dispatched, so the tile stays
                // a bare surfaceElevated square (the reference's behaviour).
                "artwork": i < root.albumCoverCap ? (root.artMap[r.artKey] || "") : ""
            })
        }
        return out
    }

    function trackCard(row) {
        if (!row)
            return ({})
        return {
            "id": row.id,
            "title": row.title,
            "artist": row.artist,
            "duration": row.duration,
            "artwork": root.artMap[row.artKey] || ""
        }
    }

    // Artists and labels take the same card; both carry the display name in
    // `title` (src/library_qt.rs:424, :695).
    function personCard(row) {
        if (!row)
            return ({})
        return {
            "id": row.id,
            "name": row.title,
            "artwork": root.artMap[row.artKey] || ""
        }
    }

    // ---- activation ------------------------------------------------------
    // Slint's FavoritesActions.play-track queues the whole VISIBLE list from
    // the clicked row; libraryPlayVisible is the Qt member that reproduces it.
    function playTrack(id) {
        var rows = root.trackRows
        var ids = []
        for (var i = 0; i < rows.length; i++)
            ids.push(rows[i].id)
        QbzLibrary.libraryPlayVisible(JSON.stringify(ids), id)
    }

    // ---- nav geometry ----------------------------------------------------
    // 5 tabs + the active tab's items. The clamp of `index` into the new count
    // is the bridge's (src/kiosk_nav_qt.rs:271-273).
    function publishNav() {
        var columns = root.activeTab === "tracks" ? 1
            : root.activeTab === "albums" ? 6
            : root.activeTab === "artists" ? 7
            : root.activeTab === "playlists" ? 6
            : 7
        var items = root.activeTab === "tracks" ? root.trackRows.length
            : root.activeTab === "albums" ? root.albumRows.length
            : root.activeTab === "artists" ? root.artistRows.length
            : root.activeTab === "playlists" ? root.playlistRows.length
            : root.labelRows.length
        QbzKioskNav.publishNav(5, columns, 5 + items, false)
    }

    Component.onCompleted: {
        root.publishNav()
        root.dispatchArtwork()
    }

    onActiveTabChanged: {
        root.publishNav()
        root.dispatchArtwork()
    }

    // ONE probe over the SUM of the five lengths, as the reference has — the
    // feed publishes once for all five lists.
    readonly property int navLenProbe: root.trackRows.length
        + root.albumRows.length
        + root.artistRows.length
        + root.playlistRows.length
        + root.labelRows.length

    onNavLenProbeChanged: {
        root.publishNav()
        root.dispatchArtwork()
    }

    // Enter on a TAB → the same switch the tab taps drive. Items are activated
    // by their own container, which watches the same pulse; the `index < tabs`
    // test is what keeps the two disjoint.
    Connections {
        target: QbzKioskNav

        function onActivateSeqChanged() {
            if (QbzKioskNav.navActive && QbzKioskNav.zone === "content"
                && QbzKioskNav.index >= 0
                && QbzKioskNav.index < QbzKioskNav.tabs
                && QbzKioskNav.index < root.tabIds.length)
                root.activeTab = root.tabIds[QbzKioskNav.index]
        }
    }

    // ============================ tab strip ==============================

    Rectangle {
        id: tabStrip

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: root.top
        height: 48
        color: theme.surfaceMain

        Row {
            anchors.fill: tabStrip
            anchors.leftMargin: root.pad
            anchors.rightMargin: root.pad
            spacing: 22

            Repeater {
                model: root.tabIds

                delegate: Rectangle {
                    id: tab

                    required property string modelData
                    required property int index

                    readonly property bool active: root.activeTab === tab.modelData
                    readonly property bool navFocused: QbzKioskNav.navActive
                        && QbzKioskNav.zone === "content"
                        && QbzKioskNav.index === tab.index

                    width: tabText.implicitWidth + 4
                    height: tabStrip.height
                    color: "transparent"
                    radius: theme.radiusSm
                    border.width: tab.navFocused ? 2 : 0
                    border.color: theme.accent

                    Text {
                        id: tabText

                        x: 2
                        anchors.top: tab.top
                        anchors.bottom: underline.top
                        text: root.tabLabel(tab.modelData)
                        color: tab.active ? theme.textPrimary : theme.textMuted
                        font.pixelSize: 15
                        font.weight: theme.weightSemibold
                        verticalAlignment: Text.AlignVCenter
                    }

                    Rectangle {
                        id: underline

                        x: 2
                        width: Math.max(0, tab.width - 4)
                        anchors.bottom: tab.bottom
                        height: 2
                        radius: 2
                        color: tab.active ? theme.accent : "transparent"
                    }

                    MouseArea {
                        anchors.fill: tab
                        onClicked: root.activeTab = tab.modelData
                    }
                }
            }
        }

        Rectangle {
            anchors.left: tabStrip.left
            anchors.right: tabStrip.right
            y: tabStrip.height - 1
            height: 1
            color: theme.borderSubtle
        }
    }

    // ============================= content ===============================

    Item {
        id: content

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: tabStrip.bottom
        anchors.bottom: root.bottom
        clip: true

        Loader {
            anchors.fill: content
            active: root.activeTab === "tracks"
            sourceComponent: tracksComponent
        }

        Loader {
            anchors.fill: content
            active: root.activeTab === "albums"
            sourceComponent: albumsComponent
        }

        Loader {
            anchors.fill: content
            active: root.activeTab === "artists"
            sourceComponent: artistsComponent
        }

        Loader {
            anchors.fill: content
            active: root.activeTab === "playlists"
            sourceComponent: playlistsComponent
        }

        Loader {
            anchors.fill: content
            active: root.activeTab === "labels"
            sourceComponent: labelsComponent
        }
    }

    // ---- Tracks ----------------------------------------------------------
    // A ListView windows the rows (only the visible band is mounted) — a plain
    // column of every track spiked CPU on entry, which a Pi can't afford.
    Component {
        id: tracksComponent

        ListView {
            id: tracksList

            model: root.trackRows
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive
                && QbzKioskNav.zone === "content"
                && tracksList.focusedItem >= 0
                && tracksList.focusedItem < root.trackRows.length

            // contentY is WRITTEN from this function on a settled layout
            // (Qt.callLater defers it past the current binding pass), never
            // read by a binding that feeds layout. Row pitch is 62.
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
                        root.playTrack(root.trackRows[tracksList.focusedItem].id)
                }
            }

            delegate: KioskTrackRow {
                id: trackDelegate

                required property var modelData
                required property int index

                width: tracksList.width
                track: root.trackCard(trackDelegate.modelData)
                navFocused: tracksList.itemFocused
                    && tracksList.focusedItem === trackDelegate.index
                onClicked: root.playTrack(trackDelegate.modelData.id)
            }
        }
    }

    // ---- Albums ----------------------------------------------------------
    // The grid owns its own ring, scroll-into-view and Enter.
    Component {
        id: albumsComponent

        KioskAlbumGrid {
            albums: root.albumCards
            columns: 6
            gap: 16
            pad: root.pad
            onOpen: function (id) {
                QbzAlbum.openAlbum(id)
            }
        }
    }

    // ---- Artists ---------------------------------------------------------
    Component {
        id: artistsComponent

        Flickable {
            id: artistsFlick

            readonly property int cols: 7
            readonly property real gap: 14
            readonly property real cell: (artistsFlick.width - 2 * root.pad
                - (artistsFlick.cols - 1) * artistsFlick.gap) / artistsFlick.cols
            // The +26 is the name line under the round avatar.
            readonly property real itemH: artistsFlick.cell + 26
            readonly property real pitch: artistsFlick.itemH + artistsFlick.gap

            contentWidth: artistsFlick.width
            contentHeight: Math.ceil(root.artistRows.length / artistsFlick.cols)
                * artistsFlick.pitch + root.pad
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive
                && QbzKioskNav.zone === "content"
                && artistsFlick.focusedItem >= 0
                && artistsFlick.focusedItem < root.artistRows.length
            readonly property real focusTop: root.pad
                + Math.floor(artistsFlick.focusedItem / artistsFlick.cols) * artistsFlick.pitch

            function scrollFocusIntoView() {
                if (!artistsFlick.itemFocused)
                    return
                if (artistsFlick.focusTop < artistsFlick.contentY)
                    artistsFlick.contentY = artistsFlick.focusTop - 8
                else if (artistsFlick.focusTop + artistsFlick.itemH
                         > artistsFlick.contentY + artistsFlick.height)
                    artistsFlick.contentY = artistsFlick.focusTop + artistsFlick.itemH
                        - artistsFlick.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(artistsFlick.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (artistsFlick.itemFocused)
                        QbzArtist.openArtist(root.artistRows[artistsFlick.focusedItem].id)
                }
            }

            Item {
                width: artistsFlick.contentWidth
                height: artistsFlick.contentHeight

                Repeater {
                    model: root.artistRows

                    delegate: KioskArtistCard {
                        id: artistCard

                        required property var modelData
                        required property int index

                        x: root.pad + (artistCard.index % artistsFlick.cols)
                            * (artistsFlick.cell + artistsFlick.gap)
                        y: root.pad + Math.floor(artistCard.index / artistsFlick.cols)
                            * artistsFlick.pitch
                        artSize: artistsFlick.cell
                        artist: root.personCard(artistCard.modelData)
                        navFocused: artistsFlick.itemFocused
                            && artistsFlick.focusedItem === artistCard.index
                        onClicked: function (id) {
                            QbzArtist.openArtist(id)
                        }
                    }
                }
            }
        }
    }

    // ---- Labels ----------------------------------------------------------
    // Identical geometry to Artists, same card. Slint opens the label with
    // (id, name); Qt's openLabel takes the id alone and resolves the page from
    // it, so the name argument is dropped.
    Component {
        id: labelsComponent

        Flickable {
            id: labelsFlick

            readonly property int cols: 7
            readonly property real gap: 14
            readonly property real cell: (labelsFlick.width - 2 * root.pad
                - (labelsFlick.cols - 1) * labelsFlick.gap) / labelsFlick.cols
            readonly property real itemH: labelsFlick.cell + 26
            readonly property real pitch: labelsFlick.itemH + labelsFlick.gap

            contentWidth: labelsFlick.width
            contentHeight: Math.ceil(root.labelRows.length / labelsFlick.cols)
                * labelsFlick.pitch + root.pad
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive
                && QbzKioskNav.zone === "content"
                && labelsFlick.focusedItem >= 0
                && labelsFlick.focusedItem < root.labelRows.length
            readonly property real focusTop: root.pad
                + Math.floor(labelsFlick.focusedItem / labelsFlick.cols) * labelsFlick.pitch

            function scrollFocusIntoView() {
                if (!labelsFlick.itemFocused)
                    return
                if (labelsFlick.focusTop < labelsFlick.contentY)
                    labelsFlick.contentY = labelsFlick.focusTop - 8
                else if (labelsFlick.focusTop + labelsFlick.itemH
                         > labelsFlick.contentY + labelsFlick.height)
                    labelsFlick.contentY = labelsFlick.focusTop + labelsFlick.itemH
                        - labelsFlick.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(labelsFlick.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (labelsFlick.itemFocused)
                        QbzHome.openLabel(root.labelRows[labelsFlick.focusedItem].id)
                }
            }

            Item {
                width: labelsFlick.contentWidth
                height: labelsFlick.contentHeight

                Repeater {
                    model: root.labelRows

                    delegate: KioskArtistCard {
                        id: labelCard

                        required property var modelData
                        required property int index

                        x: root.pad + (labelCard.index % labelsFlick.cols)
                            * (labelsFlick.cell + labelsFlick.gap)
                        y: root.pad + Math.floor(labelCard.index / labelsFlick.cols)
                            * labelsFlick.pitch
                        artSize: labelsFlick.cell
                        artist: root.personCard(labelCard.modelData)
                        navFocused: labelsFlick.itemFocused
                            && labelsFlick.focusedItem === labelCard.index
                        onClicked: function (id) {
                            QbzHome.openLabel(id)
                        }
                    }
                }
            }
        }
    }

    // ---- Playlists -------------------------------------------------------
    // A bespoke inline card, not KioskCard and not the desktop PlaylistCard:
    // square cover + one title line, nothing else. The cover is covers[0] —
    // the FIRST member-album cover, which is what Slint's `cover1` (collage
    // slot 0) holds — drawn as ONE crop-fitted tile, not a 2x2 mosaic.
    Component {
        id: playlistsComponent

        Flickable {
            id: plsFlick

            readonly property int cols: 6
            readonly property real gap: 16
            readonly property real cell: (plsFlick.width - 2 * root.pad
                - (plsFlick.cols - 1) * plsFlick.gap) / plsFlick.cols
            readonly property real itemH: plsFlick.cell + 42
            readonly property real pitch: plsFlick.itemH + plsFlick.gap

            contentWidth: plsFlick.width
            contentHeight: Math.ceil(root.playlistRows.length / plsFlick.cols)
                * plsFlick.pitch + root.pad
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive
                && QbzKioskNav.zone === "content"
                && plsFlick.focusedItem >= 0
                && plsFlick.focusedItem < root.playlistRows.length
            readonly property real focusTop: root.pad
                + Math.floor(plsFlick.focusedItem / plsFlick.cols) * plsFlick.pitch

            function scrollFocusIntoView() {
                if (!plsFlick.itemFocused)
                    return
                if (plsFlick.focusTop < plsFlick.contentY)
                    plsFlick.contentY = plsFlick.focusTop - 8
                else if (plsFlick.focusTop + plsFlick.itemH
                         > plsFlick.contentY + plsFlick.height)
                    plsFlick.contentY = plsFlick.focusTop + plsFlick.itemH
                        - plsFlick.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(plsFlick.scrollFocusIntoView)
                }

                function onActivateSeqChanged() {
                    if (plsFlick.itemFocused)
                        QbzBridge.openPlaylist(root.playlistRows[plsFlick.focusedItem].id)
                }
            }

            Item {
                width: plsFlick.contentWidth
                height: plsFlick.contentHeight

                Repeater {
                    model: root.playlistRows

                    delegate: Rectangle {
                        id: plCard

                        required property var modelData
                        required property int index

                        readonly property bool focused: plsFlick.itemFocused
                            && plsFlick.focusedItem === plCard.index
                        readonly property string cover: plCard.modelData
                            && plCard.modelData.covers
                            && plCard.modelData.covers.length > 0
                                ? plCard.modelData.covers[0] : ""

                        x: root.pad + (plCard.index % plsFlick.cols)
                            * (plsFlick.cell + plsFlick.gap)
                        y: root.pad + Math.floor(plCard.index / plsFlick.cols)
                            * plsFlick.pitch
                        width: plsFlick.cell
                        height: plsFlick.itemH
                        radius: theme.radiusSm
                        border.width: plCard.focused ? 3 : 0
                        border.color: theme.accent
                        color: plCard.focused
                            ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.12)
                            : "transparent"

                        Column {
                            width: plCard.width
                            spacing: 7

                            Rectangle {
                                id: coverTile

                                width: plCard.width
                                height: plCard.width
                                radius: theme.radiusSm
                                color: theme.surfaceElevated
                                clip: true

                                RoundedImage {
                                    anchors.fill: coverTile
                                    source: plCard.cover
                                    radius: coverTile.radius
                                    fit: "crop"
                                }
                            }

                            Text {
                                width: plCard.width
                                text: plCard.modelData && plCard.modelData.title
                                    ? plCard.modelData.title : ""
                                color: theme.textPrimary
                                font.pixelSize: 14
                                font.weight: theme.weightSemibold
                                elide: Text.ElideRight
                                wrapMode: Text.NoWrap
                                maximumLineCount: 1
                            }
                        }

                        MouseArea {
                            anchors.fill: plCard
                            onClicked: QbzBridge.openPlaylist(plCard.modelData.id)
                        }
                    }
                }
            }
        }
    }
}
