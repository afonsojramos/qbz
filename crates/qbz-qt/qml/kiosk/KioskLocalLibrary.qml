// KioskLocalLibrary — the lightweight kiosk Local Library.
// QML port of crates/qbz-ui/ui/shell/KioskLocalLibrary.slint (234 lines).
//
// Reads the SAME local documents the desktop LocalLibraryView reads (three
// sources unified in Rust: local / offline / plex), rendered with the shared
// kiosk cards. Tabs Albums / Artists / Folders / Tracks; a card tap drives
// QbzLocal directly (openAlbum routes to the local-album view in Rust).
//
// The reference's v1 shape, reproduced exactly (KioskLocalLibrary.slint:5-7):
//   - the album artwork window is reported ONCE per tab entry and never on
//     scroll, so only the first band of covers ever resolves — the Slint's
//     ALBUMS_WINDOW stays at its initial (0, 59) because the kiosk grid never
//     fires the window hook (qbz/src/local_library.rs:289);
//   - the tracks tab shows the first page only; QbzLocal.tracksLoadMore is
//     never called, exactly as the Slint kiosk never drives the load-more
//     hook (the two frontends' page sizes differ — 200 in
//     qbz/src/local_library.rs:980, 500 in src/local_state.rs:20);
//   - Folders is the FLAT album-grid representation (localFoldersJson), never
//     the tree — no rail, no folder detail pane.
//
// Track rows carry no artwork: the reference builds every local track row
// with an empty image and spawns no per-row artwork job
// (qbz/src/local_library.rs:976-978 and :1028), which is the same default the
// desktop Qt tracks tab ships with. So the tracks tab reports no artwork
// window and its 46px art tiles stay bare.
//
// Keyboard/gamepad nav: same model as KioskLibrary — the index space is the 4
// tabs followed by the active tab's items as a uniform grid (tracks = a
// 1-column list). This view PUBLISHES that geometry into QbzKioskNav; each
// container mirrors the focus index (ring + scroll-into-view) and answers the
// Enter pulse. contentY is only ever WRITTEN from a callback on a settled
// layout, never read by a layout-feeding binding (the AlbumCollectionView
// "Recursion detected" panic class).
//
// Blank is a state here. The reference reads no loading flag, no error and no
// availability flag in this file, and has no empty-tab copy — so a loading,
// failed, unavailable or empty tab renders the strip and nothing else.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    color: "transparent"

    // KioskLocalLibrary.slint:58.
    property real pad: 16

    QbzTheme { id: theme }

    function t(s) {
        return QbzSession.tr(s, QbzSession.trRev)
    }

    // ---------------------------- tab state -------------------------------
    // The Slint tab is the global LocalLibraryState.active-tab; the Qt port
    // holds it view-locally, exactly like the desktop LocalLibraryView
    // (qml/views/LocalLibraryView.qml:63), and drives QbzLocal.loadTab on
    // mount and on every switch (LocalLibraryView.qml:224, :229).
    property string activeTab: "albums"

    readonly property var tabIds: ["albums", "artists", "folders", "tracks"]

    function tabLabel(id) {
        if (id === "albums")
            return root.t("Albums")
        if (id === "artists")
            return root.t("Artists")
        if (id === "folders")
            return root.t("Folders")
        return root.t("Tracks")
    }

    function selectTab(id) {
        root.activeTab = id
    }

    onActiveTabChanged: {
        QbzLocal.loadTab(root.activeTab)
        root.publishNav()
        root.requestArtwork()
    }

    Component.onCompleted: {
        QbzLocal.loadTab(root.activeTab)
        root.publishNav()
        root.requestArtwork()
    }

    // ---------------------------- documents -------------------------------
    // A raw JSON.parse in a binding throws on the pre-publish frame and takes
    // the view down with it; every document goes through the guarded parse.
    function parseDoc(json, fallback) {
        if (json === "")
            return fallback
        try {
            return JSON.parse(json)
        } catch (e) {
            console.warn("[qbz-qt] kiosk local: bad document — " + e)
            return fallback
        }
    }

    readonly property var albums: root.parseDoc(QbzLocal.localAlbumsJson, [])
    readonly property var artists: root.parseDoc(QbzLocal.localArtistsJson, [])
    readonly property var folders: root.parseDoc(QbzLocal.localFoldersJson, [])
    readonly property var tracks: root.parseDoc(QbzLocal.localTracksJson, [])

    onAlbumsChanged: root.requestArtwork()
    onArtistsChanged: root.requestArtwork()
    onFoldersChanged: root.requestArtwork()

    // ---------------------------- artwork ---------------------------------
    // Covers are id-keyed: the view reports the artKeys it wants and Rust
    // answers one localArtworkReady per resolved key.
    //
    // The band is 60 because that is what the reference resolves: both
    // windows are initialised to (0, 59) and the kiosk moves neither
    // (qbz/src/favorites.rs:929, qbz/src/local_library.rs:289). Cards past it
    // keep the bare surfaceElevated square, as they do on the panel today.
    readonly property int coverBand: 60

    property var artMap: ({})
    property var _artInbox: ({})

    function artPathOf(key) {
        return root.artMap[key] || ""
    }

    // Arrivals stream in one at a time. Unlike the desktop grids — whose
    // cells bind an artSource per row and simply re-evaluate — the kiosk card
    // takes its cover INSIDE the album object, so every rebind rebuilds the
    // grid's model array and remounts its cards. Arrivals are therefore
    // coalesced into one rebind per 120ms band instead of one per frame: the
    // covers still appear progressively, at a fraction of the remount cost a
    // Pi would otherwise pay during exactly the seconds the grid is filling.
    Timer {
        id: artFlush
        interval: 120
        repeat: false
        onTriggered: {
            // A rebind needs a NEW object reference — a same-ref assignment
            // is not a change in QML.
            root.artMap = Object.assign({}, root.artMap, root._artInbox)
            root._artInbox = ({})
        }
    }

    Connections {
        target: QbzLocal

        function onLocalArtworkReady(key, path) {
            root._artInbox[key] = path
            if (!artFlush.running)
                artFlush.start()
        }
    }

    /// Report the active tab's artwork window. Called on mount, on every tab
    /// switch and when the active document lands — never on scroll, which is
    /// the whole of the "no album window" limit.
    function requestArtwork() {
        var rows = root.activeTab === "albums" ? root.albums : root.activeTab === "folders" ? root.folders : root.activeTab === "artists" ? root.artists : []
        // Artist portraits are push-for-all in the reference (every merged
        // row goes through the image pass, qbz/src/local_library.rs:3694),
        // so the artists tab asks for its whole set; the two album grids ask
        // for the first band only.
        var cap = root.activeTab === "artists" ? rows.length : Math.min(rows.length, root.coverBand)
        var keys = []
        var seen = ({})
        for (var i = 0; i < cap; i++) {
            var k = rows[i] ? rows[i].artKey : ""
            // A key already resolved (or already in flight) costs Rust a stat
            // per re-request, so it is never sent twice.
            if (!k || seen[k] || root.artMap[k] !== undefined || root._artInbox[k] !== undefined)
                continue
            seen[k] = true
            keys.push(k)
        }
        if (keys.length > 0)
            QbzLocal.artworkWindow(JSON.stringify(keys))
    }

    /// AlbumRow (src/local_rows.rs:26) -> the KioskCard album object, with the
    /// remote artKey already resolved to a local cover path.
    function cardsFrom(rows) {
        var out = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            out.push({
                "id": r.id,
                "title": r.title,
                "artist": r.artist,
                "qualityTier": r.qualityTier,
                "artwork": root.artPathOf(r.artKey)
            })
        }
        return out
    }

    // Only the mounted tab's cards are materialised: the inactive arm stays
    // an empty array so an artMap rebind cannot walk a list nobody shows.
    readonly property var albumCards: root.activeTab === "albums" ? root.cardsFrom(root.albums) : []
    readonly property var folderCards: root.activeTab === "folders" ? root.cardsFrom(root.folders) : []

    // ---------------------------- nav publish -----------------------------
    // KioskLocalLibrary.slint:61-78. The index clamp lives in the model
    // (publishNav applies it), so this only carries the geometry.
    function publishNav() {
        var columns
        var count
        if (root.activeTab === "albums") {
            columns = 6
            count = 4 + root.albums.length
        } else if (root.activeTab === "artists") {
            columns = 7
            count = 4 + root.artists.length
        } else if (root.activeTab === "folders") {
            columns = 6
            count = 4 + root.folders.length
        } else {
            columns = 1
            count = 4 + root.tracks.length
        }
        QbzKioskNav.publishNav(4, columns, count, false)
    }

    // KioskLocalLibrary.slint:86-92 — ONE probe over the four lengths, not
    // four handlers.
    readonly property int navLenProbe: root.albums.length + root.artists.length + root.folders.length + root.tracks.length
    onNavLenProbeChanged: root.publishNav()

    // Enter on a tab → the same switch the tab taps drive
    // (KioskLocalLibrary.slint:95-109). The `index < tabs` test is what keeps
    // this handler and the mounted container's disjoint: both see the pulse.
    Connections {
        target: QbzKioskNav

        function onActivateSeqChanged() {
            if (!QbzKioskNav.navActive || QbzKioskNav.zone !== "content")
                return
            if (QbzKioskNav.index >= QbzKioskNav.tabs)
                return
            var id = root.tabIds[QbzKioskNav.index]
            if (id !== undefined)
                root.selectTab(id)
        }
    }

    // ------------------------------ tab strip -----------------------------
    Rectangle {
        id: tabStrip

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: root.top
        height: 48
        color: theme.surfaceMain

        Row {
            id: tabsRow

            anchors.left: tabStrip.left
            anchors.leftMargin: root.pad
            anchors.top: tabStrip.top
            anchors.bottom: tabStrip.bottom
            spacing: 22

            Repeater {
                model: root.tabIds

                delegate: Rectangle {
                    id: tab

                    required property string modelData
                    required property int index

                    readonly property bool active: root.activeTab === tab.modelData
                    readonly property bool navFocused: QbzKioskNav.navActive && QbzKioskNav.zone === "content" && QbzKioskNav.index === tab.index

                    width: tabText.implicitWidth + 4
                    height: tabsRow.height
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

                        anchors.left: tab.left
                        anchors.leftMargin: 2
                        anchors.right: tab.right
                        anchors.rightMargin: 2
                        anchors.bottom: tab.bottom
                        height: 2
                        radius: 2
                        color: tab.active ? theme.accent : "transparent"
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.selectTab(tab.modelData)
                    }
                }
            }
        }

        // KioskLocalLibrary.slint:127-131 — full-width hairline on the last
        // row of the strip.
        Rectangle {
            anchors.left: tabStrip.left
            anchors.right: tabStrip.right
            y: tabStrip.height - 1
            height: 1
            color: theme.borderSubtle
        }
    }

    // ------------------------------- content ------------------------------
    Item {
        id: content

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: tabStrip.bottom
        anchors.bottom: root.bottom
        clip: true

        Loader {
            anchors.fill: parent
            active: root.activeTab === "albums"
            sourceComponent: albumsComponent
        }

        Loader {
            anchors.fill: parent
            active: root.activeTab === "folders"
            sourceComponent: foldersComponent
        }

        Loader {
            anchors.fill: parent
            active: root.activeTab === "artists"
            sourceComponent: artistsComponent
        }

        Loader {
            anchors.fill: parent
            active: root.activeTab === "tracks"
            sourceComponent: tracksComponent
        }
    }

    // KioskLocalLibrary.slint:138-143. The grid owns its own ring, its
    // scroll-into-view and the Enter pulse for its cards.
    Component {
        id: albumsComponent

        KioskAlbumGrid {
            albums: root.albumCards
            pad: root.pad
            onOpen: function (id) {
                QbzLocal.openAlbum(id)
            }
        }
    }

    // KioskLocalLibrary.slint:145-150 — the SAME grid over the flat folder
    // document, opening through the same album route.
    Component {
        id: foldersComponent

        KioskAlbumGrid {
            albums: root.folderCards
            pad: root.pad
            onOpen: function (id) {
                QbzLocal.openAlbum(id)
            }
        }
    }

    // KioskLocalLibrary.slint:152-198 — absolute-positioned round cards in a
    // vertical Flickable, 7 columns, 14px gap, a 26px name band under each
    // avatar.
    Component {
        id: artistsComponent

        Flickable {
            id: arts

            readonly property int cols: 7
            readonly property real gap: 14
            readonly property real cell: (arts.width - 2 * root.pad - (arts.cols - 1) * arts.gap) / arts.cols
            readonly property real itemH: arts.cell + 26
            readonly property real pitch: arts.itemH + arts.gap

            contentWidth: arts.width
            contentHeight: Math.ceil(root.artists.length / arts.cols) * arts.pitch + root.pad
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive && QbzKioskNav.zone === "content" && arts.focusedItem >= 0 && arts.focusedItem < root.artists.length
            readonly property real focusTop: root.pad + Math.floor(arts.focusedItem / arts.cols) * arts.pitch

            // contentY is WRITTEN here on a settled layout (Qt.callLater
            // defers it past the current binding pass), never read by a
            // binding that feeds layout.
            function scrollFocusIntoView() {
                if (!arts.itemFocused)
                    return
                if (arts.focusTop < arts.contentY)
                    arts.contentY = arts.focusTop - 8
                else if (arts.focusTop + arts.itemH > arts.contentY + arts.height)
                    arts.contentY = arts.focusTop + arts.itemH - arts.height + 8
            }

            Connections {
                target: QbzKioskNav

                function onIndexChanged() {
                    Qt.callLater(arts.scrollFocusIntoView)
                }

                // The tap path passes the artist NAME as the id, and so does
                // this one — local rows have no catalog id.
                function onActivateSeqChanged() {
                    if (arts.itemFocused)
                        QbzLocal.openArtistByName(root.artists[arts.focusedItem].name)
                }
            }

            Item {
                width: arts.contentWidth
                height: arts.contentHeight

                Repeater {
                    model: root.artists

                    // Same in-window mounting as KioskAlbumGrid
                    // (qml/kiosk/KioskAlbumGrid.qml:6-12): cheap positioned
                    // slots for every artist, the card itself only for the
                    // band around the viewport. A local library reaches
                    // thousands of artists, and mounting every avatar on
                    // entry is the CPU spike the kiosk cards exist to avoid.
                    delegate: Item {
                        id: slot

                        required property var modelData
                        required property int index

                        x: root.pad + (slot.index % arts.cols) * (arts.cell + arts.gap)
                        y: root.pad + Math.floor(slot.index / arts.cols) * arts.pitch
                        width: arts.cell
                        height: arts.itemH

                        readonly property bool inWindow: (slot.y + slot.height >= arts.contentY - 240) && (slot.y <= arts.contentY + arts.height + 240)

                        Component {
                            id: artistCardComponent

                            // ArtistRow (src/local_rows.rs:91) carries a name
                            // and no id — the reference's display-name has no
                            // Qt counterpart, so the name is both the label
                            // and the route key.
                            KioskArtistCard {
                                artSize: arts.cell
                                artist: ({
                                    "id": slot.modelData.name,
                                    "name": slot.modelData.name,
                                    "artwork": root.artPathOf(slot.modelData.artKey)
                                })
                                navFocused: arts.itemFocused && arts.focusedItem === slot.index
                                onClicked: function (id) {
                                    QbzLocal.openArtistByName(id)
                                }
                            }
                        }

                        Loader {
                            active: slot.inWindow
                            sourceComponent: artistCardComponent
                        }
                    }
                }
            }
        }
    }

    // KioskLocalLibrary.slint:200-231 — a windowing list of 62px rows.
    Component {
        id: tracksComponent

        ListView {
            id: tracksList

            model: root.tracks
            clip: true
            spacing: 0
            boundsBehavior: Flickable.StopAtBounds

            readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
            readonly property bool itemFocused: QbzKioskNav.navActive && QbzKioskNav.zone === "content" && tracksList.focusedItem >= 0 && tracksList.focusedItem < root.tracks.length

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
                        root.playTrack(root.tracks[tracksList.focusedItem].id)
                }
            }

            delegate: KioskTrackRow {
                id: trackRow

                required property var modelData
                required property int index

                width: tracksList.width
                track: trackRow.modelData
                navFocused: tracksList.itemFocused && tracksList.focusedItem === trackRow.index
                onClicked: root.playTrack(trackRow.modelData.id)
            }
        }
    }

    /// The loaded page becomes the queue in render order, starting at the
    /// clicked row — the Qt shape of the reference's track-action "play".
    function playTrack(id) {
        var ids = []
        for (var i = 0; i < root.tracks.length; i++)
            ids.push(root.tracks[i].id)
        QbzLocal.playTracksVisible(JSON.stringify(ids), id)
    }
}
