// Local Library — QML port of crates/qbz-ui/ui/locallibrary/
// LocalLibraryView.slint (the shipping behaviour) with the Tauri
// LocalLibraryView / LocalLibraryFolderTree / LocalLibraryFolderDetail /
// LocalLibraryFolderAlbumView components as the visual reference.
//
// CHROME (identical to FavoritesView / LibraryView, per the Slint header):
//   row 1  "Local Library" title + settings gear
//   row 2  segmented tab bar (Albums / Artists / Folders / Tracks, with
//          count badges) on the left, the per-tab toolbar floated right
//   1px divider, then ONE scrolling content area.
//
// DATA: QbzLocal publishes one JSON document per surface (the transport
// rationale in library_qt.rs — the QML side parses once and derives search /
// sort / grouping in JS). Nothing here re-implements the local-library
// backend: qbz-library owns the scan, the album identity (folder vs
// metadata) and every query; local_library_qt.rs only maps rows.
//
// PERFORMANCE (this is the surface with the documented Slint freeze at 16K+
// tracks):
//   - the Tracks tab is SERVER-paginated (500/page, appended on scroll);
//   - every list/grid is a windowing view (GridView / ListView), never a
//     Repeater over the full array;
//   - the folder tree is a FLAT array windowed by a ListView — the Tauri
//     recursive component is deliberately not reproduced, and levels are
//     fetched lazily on expand;
//   - artwork is id-keyed (artKey), requested only for the MOUNTED window,
//     resolved to a 256px thumbnail in Rust, and evicted from `artMap` once
//     it falls a full window away from the viewport.
//
// POC-NOTEs (deliberate cuts, named for the effort report): multi-select +
// bulk bars, ephemeral folders, the tag editor, A-Z alpha jump strips, the
// Plex union, the rail's drag-to-resize handle and the per-tab scroll
// restore are out of scope.

import QtQuick
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../rows"
import "../theme"

Rectangle {
    id: root

    // Transparent while the ambient background is active (the frosted
    // content panel shows through — AppShell's contentFrame owns the fill).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    // Round to the AppShell content-frame bezel (QML clips are rectangular).
    radius: 12

    QbzTheme { id: theme }

    // ============================ state ==================================
    // Slint defaults, verbatim (state.slint LocalLibraryState).
    property string activeTab: "albums"

    // Albums tab.
    property string albumsSearch: ""
    property string albumsSort: "artist-asc"
    property string albumsGroup: "off"      // off | alpha | artist
    property string albumsView: "grid"      // grid | list

    // Folders tab.
    property string foldersMode: "tree"     // flat | tree
    property string foldersSearch: ""
    property string foldersSort: "artist-asc"
    property string foldersGridView: "grid"
    property string treeSearch: ""
    property string selectedFolder: ""
    property string folderDetailSearch: ""
    property string folderDetailView: "grid"
    property int treeRailWidth: 272

    // Artists tab.
    property string artistsSearch: ""
    property string selectedArtist: ""

    // Tracks tab.
    property string tracksSearch: ""

    // ---------------------------- documents ------------------------------
    function parseDoc(json, fallback) {
        if (json === "") return fallback
        try {
            return JSON.parse(json)
        } catch (e) {
            console.warn("[qbz-qt] local: bad document — " + e)
            return fallback
        }
    }
    readonly property var counts: parseDoc(QbzLocal.localCountsJson, ({}))
    readonly property var albums: parseDoc(QbzLocal.localAlbumsJson, [])
    readonly property var artists: parseDoc(QbzLocal.localArtistsJson, [])
    readonly property var folders: parseDoc(QbzLocal.localFoldersJson, [])
    readonly property var tree: parseDoc(QbzLocal.localTreeJson, [])
    readonly property var tracks: parseDoc(QbzLocal.localTracksJson, [])
    readonly property var folderDetail: parseDoc(QbzLocal.localDetailJson, null)
    readonly property var albumDetail: parseDoc(QbzLocal.localAlbumJson, null)

    // Decoded-cover map {artKey: "file://…"} — fed by the id-keyed signal.
    property var artMap: ({})

    Connections {
        target: QbzLocal
        function onLocalArtworkReady(key, path) {
            var m = root.artMap
            m[key] = path
            // A rebind needs a NEW object reference (same-ref assignment is
            // not a change in QML).
            root.artMap = Object.assign({}, m)
        }
    }

    // Mount: load the default tab. Tab switches load on demand (each tab is
    // one query; the Albums/Folders/Artists sets are bounded).
    Component.onCompleted: QbzLocal.loadTab(root.activeTab)
    onActiveTabChanged: QbzLocal.loadTab(activeTab)

    // Detail panes: their cover sets are SMALL and fully mounted, so the
    // whole set is one window report (the grids/lists do the scrolling
    // windowing themselves).
    onFolderDetailChanged: {
        if (folderDetail) queueWindowReport(folderDetail.subfolders, 0,
                                            folderDetail.subfolders.length - 1)
    }
    onAlbumDetailChanged: {
        if (albumDetail) queueWindowReport([albumDetail.album], 0, 0)
    }

    // ---------------------- derived (JS, per-tab) -------------------------
    function sortRows(rows, key) {
        var field = key.indexOf("year") === 0 ? "year"
                  : key.indexOf("title") === 0 ? "title" : "artist"
        var desc = key.slice(-4) === "desc"
        var out = rows.slice()
        out.sort(function (a, b) {
            var av = (a[field] || "").toString().toLowerCase()
            var bv = (b[field] || "").toString().toLowerCase()
            if (av === bv) {
                var at = (a.title || "").toLowerCase()
                var bt = (b.title || "").toLowerCase()
                return at < bt ? -1 : at > bt ? 1 : 0
            }
            return av < bv ? -1 : 1
        })
        if (desc) out.reverse()
        return out
    }

    function filterRows(rows, needle) {
        var q = needle.trim().toLowerCase()
        if (q === "") return rows
        var out = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            if ((r.title || "").toLowerCase().indexOf(q) >= 0
                || (r.artist || "").toLowerCase().indexOf(q) >= 0) out.push(r)
        }
        return out
    }

    readonly property var albumsVisible: sortRows(filterRows(albums, albumsSearch), albumsSort)
    readonly property var foldersVisible: sortRows(filterRows(folders, foldersSearch), foldersSort)

    readonly property var artistsVisible: {
        var q = artistsSearch.trim().toLowerCase()
        if (q === "") return artists
        var out = []
        for (var i = 0; i < artists.length; i++) {
            if ((artists[i].name || "").toLowerCase().indexOf(q) >= 0) out.push(artists[i])
        }
        return out
    }

    // The selected artist's albums — client-side over the loaded album set
    // (`allArtists` is the comma-joined contributor list the DB aggregates).
    readonly property var artistAlbums: {
        if (selectedArtist === "") return []
        var needle = selectedArtist.toLowerCase()
        var out = []
        for (var i = 0; i < albums.length; i++) {
            var a = albums[i]
            if ((a.artist || "").toLowerCase() === needle
                || (a.allArtists || "").toLowerCase().indexOf(needle) >= 0) out.push(a)
        }
        return out
    }

    readonly property var detailSubfolders: {
        if (!folderDetail) return []
        var q = folderDetailSearch.trim().toLowerCase()
        if (q === "") return folderDetail.subfolders
        var out = []
        for (var i = 0; i < folderDetail.subfolders.length; i++) {
            if ((folderDetail.subfolders[i].name || "").toLowerCase().indexOf(q) >= 0)
                out.push(folderDetail.subfolders[i])
        }
        return out
    }

    // --------------------- windowed artwork ------------------------------
    // Report the mounted window as artKeys and prune covers a full window
    // away. Identical policy to LibraryView (the Slint eviction, QML-side).
    function reportWindow(rows, first, last) {
        if (!rows || rows.length === 0) return
        last = Math.min(last, rows.length - 1)
        first = Math.max(0, first)
        if (first > last) return
        var keys = []
        var keep = {}
        var span = last - first + 1
        var lo = Math.max(0, first - span)
        var hi = Math.min(rows.length - 1, last + span)
        var i
        for (i = lo; i <= hi; i++) if (rows[i].artKey) keep[rows[i].artKey] = true
        for (i = first; i <= last; i++) if (rows[i].artKey) keys.push(rows[i].artKey)
        var m = root.artMap
        var changed = false
        for (var key in m) {
            if (!keep[key]) { delete m[key]; changed = true }
        }
        if (changed) root.artMap = Object.assign({}, m)
        if (keys.length > 0) QbzLocal.artworkWindow(JSON.stringify(keys))
    }

    // Debounced reporting (180ms — the LibraryView throttle).
    Timer {
        id: windowDebounce
        interval: 180
        property var pendingRows: []
        property int pendingFirst: 0
        property int pendingLast: 0
        onTriggered: root.reportWindow(pendingRows, pendingFirst, pendingLast)
    }
    function queueWindowReport(rows, first, last) {
        windowDebounce.pendingRows = rows
        windowDebounce.pendingFirst = first
        windowDebounce.pendingLast = last
        windowDebounce.restart()
    }
    /// Window report for a wrapping card grid.
    function gridReport(view, rows) {
        if (!view.visible || view.cellHeight <= 0) return
        var cols = Math.max(1, Math.floor(view.width / view.cellWidth))
        var firstRow = Math.max(0, Math.floor(view.contentY / view.cellHeight) - 1)
        var lastRow = Math.ceil((view.contentY + view.height) / view.cellHeight) + 1
        queueWindowReport(rows, firstRow * cols, lastRow * cols - 1)
    }
    function listReport(view, rows, rowHeight) {
        if (!view.visible) return
        var first = Math.max(0, Math.floor(view.contentY / rowHeight) - 4)
        var last = Math.ceil((view.contentY + view.height) / rowHeight) + 4
        queueWindowReport(rows, first, last)
    }

    // ============================ components =============================

    // Compact toolbar search (the Slint rail/toolbar input: 30px, r6,
    // elevated, accent border on focus).
    component LocalSearch: Rectangle {
        id: searchRoot
        property string placeholder: ""
        property alias text: searchInput.text
        property int boxWidth: 200
        signal edited(string value)
        width: boxWidth
        height: 30
        radius: 6
        color: theme.surfaceElevated
        border.width: 1
        border.color: searchInput.activeFocus ? theme.accent : theme.borderSubtle
        clip: true
        Row {
            anchors.fill: parent
            anchors.leftMargin: 9
            anchors.rightMargin: 8
            spacing: 6
            QbzIcon {
                name: "search"
                width: 13
                height: 13
                anchors.verticalCenter: parent.verticalCenter
                tintName: "muted"
            }
            TextInput {
                id: searchInput
                width: parent.width - 19
                height: parent.height
                color: theme.textPrimary
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                clip: true
                onTextEdited: searchRoot.edited(text)
                Text {
                    visible: searchInput.text === ""
                    anchors.fill: parent
                    text: searchRoot.placeholder
                    color: theme.textMuted
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
        }
    }

    // Grid / list segmented toggle (ViewToggle.slint).
    component ViewToggle: Rectangle {
        id: viewToggleRoot
        property string mode: "grid"
        signal setMode(string value)
        width: 60
        height: 30
        radius: 6
        color: theme.surfaceElevated
        Row {
            anchors.centerIn: parent
            spacing: 2
            Repeater {
                model: [
                    { "id": "grid", "icon": "layout-grid" },
                    { "id": "list", "icon": "list" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: 26
                    height: 24
                    radius: 4
                    color: modelData.id === viewToggleRoot.mode ? theme.surfaceMain
                         : vtArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: modelData.icon
                        width: 14
                        height: 14
                        anchors.centerIn: parent
                        tintName: modelData.id === viewToggleRoot.mode ? "accent" : "secondary"
                    }
                    MouseArea {
                        id: vtArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: viewToggleRoot.setMode(modelData.id)
                    }
                }
            }
        }
    }

    // Flat / Tree segmented toggle (ModeToggle.slint) — Folders tab only.
    component ModeToggle: Rectangle {
        id: modeToggleRoot
        property string mode: "tree"
        signal setMode(string value)
        width: 60
        height: 30
        radius: 6
        color: theme.surfaceElevated
        Row {
            anchors.centerIn: parent
            spacing: 2
            Repeater {
                model: [
                    { "id": "flat", "icon": "rows-3" },
                    { "id": "tree", "icon": "list-ordered" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: 26
                    height: 24
                    radius: 4
                    color: modelData.id === modeToggleRoot.mode ? theme.surfaceMain
                         : mtArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: modelData.icon
                        width: 14
                        height: 14
                        anchors.centerIn: parent
                        tintName: modelData.id === modeToggleRoot.mode ? "accent" : "secondary"
                    }
                    MouseArea {
                        id: mtArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: modeToggleRoot.setMode(modelData.id)
                    }
                }
            }
        }
    }

    // One flattened tree row (TreeRow.slint): 26px, depth indent 16px,
    // chevron with its OWN hit target, folder/track glyph, name. The
    // selection cue is the 3px accent bar (ADR-008: never a pill).
    component TreeRow: Rectangle {
        id: treeRowRoot
        property var node: ({})
        property bool selected: false
        signal toggled()
        signal activated()
        height: 26
        radius: 6
        color: selected ? theme.surfaceElevated
             : trArea.containsMouse ? theme.surfaceHover : "transparent"

        MouseArea {
            id: trArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: treeRowRoot.node.isFolder ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (treeRowRoot.node.isFolder) treeRowRoot.activated()
        }
        Rectangle {
            visible: treeRowRoot.selected
            x: 0
            y: 4
            width: 3
            height: parent.height - 8
            radius: 1.5
            color: theme.accent
        }
        Row {
            anchors.fill: parent
            anchors.leftMargin: 8 + treeRowRoot.node.depth * 16
            anchors.rightMargin: 10
            spacing: 6
            // Chevron — its own hit target so toggling never selects.
            Item {
                width: 18
                height: parent.height
                QbzIcon {
                    visible: treeRowRoot.node.canExpand === true
                    name: treeRowRoot.node.expanded ? "chevron-down" : "chevron-right"
                    width: 11
                    height: 11
                    anchors.centerIn: parent
                    tintName: "muted"
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: treeRowRoot.node.canExpand === true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: treeRowRoot.toggled()
                }
            }
            QbzIcon {
                name: treeRowRoot.node.isFolder
                    ? (treeRowRoot.node.expanded ? "folder-open" : "folder") : "music"
                width: 14
                height: 14
                anchors.verticalCenter: parent.verticalCenter
                tintName: treeRowRoot.node.isFolder ? "accent" : "muted"
            }
            Text {
                width: Math.max(0, parent.width - 50 - treeRowRoot.node.depth * 16)
                height: parent.height
                text: treeRowRoot.node.segment || ""
                color: treeRowRoot.selected ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                font.weight: treeRowRoot.selected ? theme.weightSemibold : theme.weightRegular
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
    }

    // Subfolder cover card (FolderSubcard.slint / Tauri `.subfolder-card`):
    // bordered card, square cover with a folder placeholder, name + the
    // recursive track count.
    component FolderSubcard: Rectangle {
        id: subcardRoot
        property var item: ({})
        signal opened()
        radius: 6
        border.width: 1
        border.color: fsArea.containsMouse ? theme.accent : theme.borderSubtle
        color: fsArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
        Column {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 6
            Rectangle {
                width: parent.width
                height: parent.width
                radius: 4
                color: theme.surfaceMain
                clip: true
                QbzIcon {
                    name: "folder"
                    width: 32
                    height: 32
                    anchors.centerIn: parent
                    tintName: "muted"
                }
                RoundedImage {
                    anchors.fill: parent
                    source: root.artMap[subcardRoot.item.artKey] || ""
                    radius: 4
                }
            }
            Text {
                width: parent.width
                text: subcardRoot.item.name || ""
                color: theme.textPrimary
                font.pixelSize: 13
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: (subcardRoot.item.trackCount || 0) + " "
                    + QbzSession.tr("tracks", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: fsArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: subcardRoot.opened()
        }
    }

    // Local track row — the SHARED TrackRow with the local arms (no heart,
    // no download slot, no catalog "go to" entries, no drag source), plus
    // the always-visible source glyph the Slint's `show-source: true`
    // renders. The glyph lives OUTSIDE the shared row so TrackRow keeps one
    // column layout for every surface.
    component LocalTrackRow: Item {
        id: localRowRoot
        property var item: ({})
        property int number: 0
        signal playRequested()
        signal enqueueRequested(string mode)
        height: 50
        TrackRow {
            id: sharedRow
            width: localRowRoot.width - 26
            item: localRowRoot.item
            number: localRowRoot.number
            // No per-row cover decode on the library-scale surfaces — the
            // Slint gates this on AppearanceState.local-library-track-artwork
            // for exactly the freeze reason, and its default is off.
            showArtwork: false
            showAlbum: true
            showFavorite: false
            showDownload: false
            menuShowGoTo: false
            menuShowFavorite: false
            draggable: false
            qualityStyle: "icon"
            onPlayRequested: localRowRoot.playRequested()
            onEnqueueRequested: function (m) { localRowRoot.enqueueRequested(m) }
        }
        QbzIcon {
            visible: localRowRoot.item.source === "local"
                || localRowRoot.item.source === "plex"
                || localRowRoot.item.source === "offline"
            name: localRowRoot.item.source === "offline" ? "cloud-download" : "hard-drive"
            width: 14
            height: 14
            x: localRowRoot.width - 20
            anchors.verticalCenter: parent.verticalCenter
            tintName: localRowRoot.item.source === "plex" ? "accent" : "muted"
        }
    }

    // Album grid shared by Albums / Folders-flat / Artists-detail. `rows` is
    // the derived array; every card routes its actions to the LOCAL bridge
    // (localMode) because the ids are group keys, not catalog ids.
    component LocalAlbumGrid: GridView {
        id: gridRoot
        property var rows: []
        model: rows
        cellWidth: 220
        cellHeight: 266
        cacheBuffer: 266 * 2
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        onContentYChanged: root.gridReport(gridRoot, gridRoot.rows)
        onRowsChanged: root.gridReport(gridRoot, gridRoot.rows)
        onWidthChanged: root.gridReport(gridRoot, gridRoot.rows)
        Component.onCompleted: root.gridReport(gridRoot, gridRoot.rows)
        delegate: Item {
            required property var modelData
            width: 200
            height: 246
            AlbumCard {
                localMode: true
                albumId: modelData.id
                title: modelData.title
                artist: modelData.artist
                artistId: ""
                genre: ""
                year: modelData.year
                qualityTier: modelData.qualityTier
                artSource: root.artMap[modelData.artKey] || ""
                source: modelData.source
                onOpenRequested: {
                    root.selectedFolder = ""
                    QbzLocal.openAlbum(modelData.id)
                }
                onPlayRequested: QbzLocal.playAlbum(modelData.id, false)
                onEnqueueRequested: function (m) {
                    QbzLocal.enqueue("album", modelData.id, m)
                }
            }
        }
    }

    // Compact album row for the list view mode (AlbumCollectionView's list
    // arm): 56px, cover + title/artist + year + track count + quality.
    component LocalAlbumRow: Rectangle {
        id: albumRowRoot
        property var item: ({})
        height: 56
        radius: 6
        color: larArea.containsMouse ? theme.surfaceHover : "transparent"
        MouseArea {
            id: larArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: QbzLocal.openAlbum(albumRowRoot.item.id)
            onDoubleClicked: QbzLocal.playAlbum(albumRowRoot.item.id, false)
        }
        Row {
            anchors.fill: parent
            anchors.leftMargin: 10
            anchors.rightMargin: 12
            spacing: 12
            Rectangle {
                width: 40
                height: 40
                anchors.verticalCenter: parent.verticalCenter
                radius: 4
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.artMap[albumRowRoot.item.artKey] || ""
                    radius: 4
                }
            }
            Column {
                width: parent.width - 40 - 70 - 90 - 100 - 4 * 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: albumRowRoot.item.title || ""
                    color: theme.textPrimary
                    font.pixelSize: 14
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: albumRowRoot.item.artist || ""
                    color: theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
            Text {
                width: 70
                anchors.verticalCenter: parent.verticalCenter
                text: albumRowRoot.item.year || ""
                color: theme.textMuted
                font.pixelSize: 12
            }
            Text {
                width: 90
                anchors.verticalCenter: parent.verticalCenter
                text: (albumRowRoot.item.trackCount || 0) + " "
                    + QbzSession.tr("tracks", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 12
            }
            Item {
                width: 100
                height: parent.height
                QualityMini {
                    tier: albumRowRoot.item.qualityTier || ""
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }

    // Centered empty / loading / no-match body used by every tab.
    component CenterNote: Text {
        anchors.centerIn: parent
        color: theme.textMuted
        font.pixelSize: theme.fontBody
        horizontalAlignment: Text.AlignHCenter
    }

    // ============================ view ===================================

    Column {
        anchors.fill: parent
        spacing: 0

        // ---- Row 1: title + gear (+ the tab bar sits on row 2) ----------
        Item {
            width: parent.width
            height: 46
            Row {
                x: 32
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("Local Library", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontSection
                    font.weight: theme.weightBold
                }
                QbzCircleAction {
                    name: "settings-2"
                    anchors.verticalCenter: parent.verticalCenter
                    // Folder management, scan and maintenance do NOT live in
                    // this view (Slint header note) — the gear routes out.
                    onClicked: QbzShell.navigateTo("settings")
                }
            }
        }

        // ---- Row 2: tabs (left) + per-tab toolbar (right) ---------------
        Item {
            width: parent.width
            height: 44

            QbzTabBar {
                x: 32
                anchors.verticalCenter: parent.verticalCenter
                counts: true
                underline: true
                activeId: root.activeTab
                // NOTE the `root.` prefix: QbzTabBar has its OWN `counts`
                // property (the badge arm), so an unqualified `counts` here
                // would resolve to that bool, not to this view's document.
                tabs: [
                    { "id": "albums", "label": QbzSession.tr("Albums", QbzSession.trRev), "count": root.counts.albums || 0 },
                    { "id": "artists", "label": QbzSession.tr("Artists", QbzSession.trRev), "count": root.counts.artists || 0 },
                    { "id": "folders", "label": QbzSession.tr("Folders", QbzSession.trRev), "count": root.counts.folders || 0 },
                    { "id": "tracks", "label": QbzSession.tr("Tracks", QbzSession.trRev), "count": root.counts.tracks || 0 },
                ]
                onSelected: function (id) {
                    root.activeTab = id
                    QbzLocal.closeAlbum()
                }
            }

            Row {
                x: parent.width - width - 32
                anchors.verticalCenter: parent.verticalCenter
                height: 30
                spacing: 8

                // ===== Albums toolbar =====
                Row {
                    visible: root.activeTab === "albums" && root.albumDetail === null
                    spacing: 8
                    height: parent.height
                    LocalSearch {
                        placeholder: QbzSession.tr("Search", QbzSession.trRev)
                        onEdited: function (v) { root.albumsSearch = v }
                    }
                    QbzSelect {
                        menuWidth: 150
                        height: 30
                        anchors.verticalCenter: parent.verticalCenter
                        options: [
                            QbzSession.tr("Artist A-Z", QbzSession.trRev),
                            QbzSession.tr("Artist Z-A", QbzSession.trRev),
                            QbzSession.tr("Title A-Z", QbzSession.trRev),
                            QbzSession.tr("Title Z-A", QbzSession.trRev),
                            QbzSession.tr("Year (newest)", QbzSession.trRev),
                            QbzSession.tr("Year (oldest)", QbzSession.trRev),
                        ]
                        currentIndex: ["artist-asc", "artist-desc", "title-asc",
                                       "title-desc", "year-desc", "year-asc"].indexOf(root.albumsSort)
                        onSelected: function (i) {
                            root.albumsSort = ["artist-asc", "artist-desc", "title-asc",
                                               "title-desc", "year-desc", "year-asc"][i]
                        }
                    }
                    // Album identity ("Albums by") — what one album IS. This
                    // is a QUERY change (the grouping happens in SQL), so it
                    // reloads the set; persisted, shared with the Slint app.
                    QbzSelect {
                        menuWidth: 175
                        height: 30
                        anchors.verticalCenter: parent.verticalCenter
                        options: [
                            QbzSession.tr("Albums by folder", QbzSession.trRev),
                            QbzSession.tr("Albums by metadata", QbzSession.trRev),
                        ]
                        currentIndex: QbzLocal.localAlbumMode === "metadata" ? 1 : 0
                        onSelected: function (i) {
                            QbzLocal.setAlbumMode(i === 1 ? "metadata" : "folder")
                        }
                    }
                    ViewToggle {
                        mode: root.albumsView
                        anchors.verticalCenter: parent.verticalCenter
                        onSetMode: function (v) { root.albumsView = v }
                    }
                }

                // ===== Artists toolbar =====
                LocalSearch {
                    visible: root.activeTab === "artists"
                    placeholder: QbzSession.tr("Search artists", QbzSession.trRev)
                    onEdited: function (v) { root.artistsSearch = v }
                }

                // ===== Folders toolbar =====
                Row {
                    visible: root.activeTab === "folders" && root.albumDetail === null
                    spacing: 8
                    height: parent.height
                    // Flat-only controls (the tree rail carries its own
                    // search — 1:1 with Tauri).
                    Row {
                        visible: root.foldersMode === "flat"
                        spacing: 8
                        height: parent.height
                        LocalSearch {
                            placeholder: QbzSession.tr("Search", QbzSession.trRev)
                            onEdited: function (v) { root.foldersSearch = v }
                        }
                        QbzSelect {
                            menuWidth: 150
                            height: 30
                            anchors.verticalCenter: parent.verticalCenter
                            options: [
                                QbzSession.tr("Artist A-Z", QbzSession.trRev),
                                QbzSession.tr("Artist Z-A", QbzSession.trRev),
                                QbzSession.tr("Title A-Z", QbzSession.trRev),
                                QbzSession.tr("Title Z-A", QbzSession.trRev),
                                QbzSession.tr("Year (newest)", QbzSession.trRev),
                                QbzSession.tr("Year (oldest)", QbzSession.trRev),
                            ]
                            currentIndex: ["artist-asc", "artist-desc", "title-asc",
                                           "title-desc", "year-desc", "year-asc"].indexOf(root.foldersSort)
                            onSelected: function (i) {
                                root.foldersSort = ["artist-asc", "artist-desc", "title-asc",
                                                    "title-desc", "year-desc", "year-asc"][i]
                            }
                        }
                        ViewToggle {
                            mode: root.foldersGridView
                            anchors.verticalCenter: parent.verticalCenter
                            onSetMode: function (v) { root.foldersGridView = v }
                        }
                    }
                    ModeToggle {
                        mode: root.foldersMode
                        anchors.verticalCenter: parent.verticalCenter
                        onSetMode: function (v) { root.foldersMode = v }
                    }
                }

                // ===== Tracks toolbar =====
                Row {
                    visible: root.activeTab === "tracks"
                    spacing: 8
                    height: parent.height
                    LocalSearch {
                        placeholder: QbzSession.tr("Search", QbzSession.trRev)
                        onEdited: function (v) {
                            root.tracksSearch = v
                            tracksSearchDebounce.restart()
                        }
                        // Server-side search: debounce so a keystroke does not
                        // fire a query per character.
                        Timer {
                            id: tracksSearchDebounce
                            interval: 250
                            onTriggered: QbzLocal.tracksSearch(root.tracksSearch)
                        }
                    }
                    QbzSelect {
                        menuWidth: 160
                        height: 30
                        anchors.verticalCenter: parent.verticalCenter
                        options: [
                            QbzSession.tr("Default", QbzSession.trRev),
                            QbzSession.tr("Artist A-Z", QbzSession.trRev),
                            QbzSession.tr("Artist Z-A", QbzSession.trRev),
                            QbzSession.tr("Title A-Z", QbzSession.trRev),
                            QbzSession.tr("Title Z-A", QbzSession.trRev),
                            QbzSession.tr("Year (newest)", QbzSession.trRev),
                            QbzSession.tr("Year (oldest)", QbzSession.trRev),
                            QbzSession.tr("Date added", QbzSession.trRev),
                        ]
                        currentIndex: Math.max(0, ["default", "artist-asc", "artist-desc",
                            "title-asc", "title-desc", "year-desc", "year-asc",
                            "added-desc"].indexOf(QbzLocal.localTracksSort))
                        onSelected: function (i) {
                            QbzLocal.tracksSetSort(["default", "artist-asc", "artist-desc",
                                "title-asc", "title-desc", "year-desc", "year-asc",
                                "added-desc"][i])
                        }
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ===================== SCROLLING CONTENT =========================
        Item {
            id: contentArea
            width: parent.width
            height: parent.height - 91
            clip: true

            // ---- "Nothing indexed yet" (no db / no registered folder) ----
            QbzEmptyState {
                visible: !QbzLocal.localAvailable
                anchors.centerIn: parent
                iconName: "folder-plus"
                title: QbzSession.tr("No local library yet", QbzSession.trRev)
                body: QbzSession.tr("Add a music folder to scan your local files.", QbzSession.trRev)
                actionLabel: QbzSession.tr("Open Local Library settings", QbzSession.trRev)
                onActionClicked: QbzShell.navigateTo("settings")
            }

            // ================= LOCAL ALBUM DETAIL ======================
            // Replaces the browse pane while an album is open (the Slint
            // routes to a dedicated LocalAlbumView; the Qt port keeps it
            // inside this view so the tab chrome stays put).
            Item {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.albumDetail !== null

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 32
                    visible: QbzLocal.localAlbumLoading
                }

                Flickable {
                    id: albumPane
                    anchors.fill: parent
                    visible: !QbzLocal.localAlbumLoading && root.albumDetail !== null
                    clip: true
                    contentWidth: width
                    contentHeight: albumCol.height + 120
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: albumCol
                        x: 32
                        y: 16
                        width: parent.width - 64
                        spacing: 14

                        // Compact header (LocalLibraryFolderAlbumView): 160px
                        // artwork, metadata column, circular actions.
                        Row {
                            width: parent.width
                            spacing: 16
                            Rectangle {
                                width: 160
                                height: 160
                                radius: 6
                                color: theme.surfaceElevated
                                clip: true
                                QbzIcon {
                                    name: "disc-3"
                                    width: 48
                                    height: 48
                                    anchors.centerIn: parent
                                    tintName: "muted"
                                }
                                RoundedImage {
                                    anchors.fill: parent
                                    source: root.albumDetail
                                        ? (root.artMap[root.albumDetail.album.artKey] || "") : ""
                                    radius: 6
                                }
                            }
                            Column {
                                width: parent.width - 176
                                spacing: 4
                                Text {
                                    width: parent.width
                                    text: root.albumDetail ? root.albumDetail.album.title : ""
                                    color: theme.textPrimary
                                    font.pixelSize: 18
                                    font.weight: theme.weightBold
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width
                                    text: root.albumDetail ? root.albumDetail.album.artist : ""
                                    color: theme.textPrimary
                                    font.pixelSize: 14
                                    font.weight: theme.weightMedium
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width
                                    text: {
                                        if (!root.albumDetail) return ""
                                        var a = root.albumDetail.album
                                        var parts = []
                                        if (a.year !== "") parts.push(a.year)
                                        parts.push(a.trackCount + " "
                                            + QbzSession.tr("tracks", QbzSession.trRev))
                                        parts.push(a.duration)
                                        return parts.join("  •  ")
                                    }
                                    color: theme.textMuted
                                    font.pixelSize: 12
                                    elide: Text.ElideRight
                                }
                                Row {
                                    spacing: 8
                                    Rectangle {
                                        visible: root.albumDetail
                                            && root.albumDetail.album.format !== ""
                                        width: fmtText.implicitWidth + 16
                                        height: 20
                                        radius: 4
                                        color: theme.surfaceElevated
                                        Text {
                                            id: fmtText
                                            anchors.centerIn: parent
                                            text: root.albumDetail
                                                ? root.albumDetail.album.format.toUpperCase() : ""
                                            color: theme.textPrimary
                                            font.pixelSize: 11
                                            font.weight: theme.weightSemibold
                                        }
                                    }
                                    Rectangle {
                                        visible: root.albumDetail
                                            && root.albumDetail.album.qualityDetail !== ""
                                        width: detText.implicitWidth + 12
                                        height: 20
                                        radius: 4
                                        color: theme.surfaceCard
                                        Text {
                                            id: detText
                                            anchors.centerIn: parent
                                            text: root.albumDetail
                                                ? root.albumDetail.album.qualityDetail : ""
                                            color: theme.textSecondary
                                            font.pixelSize: 11
                                        }
                                    }
                                }
                                Item { width: 1; height: 4 }
                                Row {
                                    spacing: 8
                                    QbzCircleAction {
                                        name: "play-fill"
                                        primary: true
                                        onClicked: QbzLocal.playAlbum(
                                            root.albumDetail.album.id, false)
                                    }
                                    QbzCircleAction {
                                        name: "shuffle"
                                        onClicked: QbzLocal.playAlbum(
                                            root.albumDetail.album.id, true)
                                    }
                                    QbzCircleAction {
                                        name: "list-start"
                                        onClicked: QbzLocal.enqueue(
                                            "album", root.albumDetail.album.id, "next")
                                    }
                                    QbzCircleAction {
                                        name: "list-end"
                                        onClicked: QbzLocal.enqueue(
                                            "album", root.albumDetail.album.id, "queue")
                                    }
                                    QbzCircleAction {
                                        name: "chevron-left"
                                        onClicked: QbzLocal.closeAlbum()
                                    }
                                }
                            }
                        }

                        Rectangle {
                            width: parent.width
                            height: 1
                            color: theme.borderSubtle
                        }

                        // Track list. The album's track count is bounded, so
                        // a Column is correct here (the windowed views are
                        // for the library-scale surfaces).
                        Column {
                            width: parent.width
                            spacing: 0
                            Repeater {
                                model: root.albumDetail ? root.albumDetail.tracks : []
                                delegate: LocalTrackRow {
                                    required property var modelData
                                    required property int index
                                    width: albumCol.width
                                    item: modelData
                                    number: modelData.number > 0 ? modelData.number : index + 1
                                    onPlayRequested: QbzLocal.playAlbumTrack(
                                        root.albumDetail.album.id, modelData.id)
                                    onEnqueueRequested: function (m) {
                                        QbzLocal.enqueue("track", modelData.id, m)
                                    }
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
                    target: albumPane
                    visible: albumPane.visible && albumPane.contentHeight > albumPane.height
                }
            }

            // ===================== ALBUMS TAB =========================
            Item {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "albums"
                    && root.albumDetail === null

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 36
                    visible: QbzLocal.localAlbumsLoading
                }
                Column {
                    visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError !== ""
                    anchors.centerIn: parent
                    spacing: 10
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("Couldn't load your local albums.", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody
                        font.weight: theme.weightSemibold
                    }
                    SettingsButton {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("Retry", QbzSession.trRev)
                        onClicked: QbzLocal.loadTab("albums")
                    }
                }
                CenterNote {
                    visible: !QbzLocal.localAlbumsLoading && QbzLocal.localAlbumsError === ""
                        && root.albums.length === 0 && root.albumsSearch === ""
                    text: QbzSession.tr("No albums in your local library yet.", QbzSession.trRev)
                }
                CenterNote {
                    visible: !QbzLocal.localAlbumsLoading && root.albums.length > 0
                        && root.albumsVisible.length === 0
                    text: QbzSession.tr("No albums match your search.", QbzSession.trRev)
                }

                LocalAlbumGrid {
                    id: albumsGrid
                    anchors.fill: parent
                    anchors.leftMargin: 32
                    anchors.rightMargin: 32
                    anchors.topMargin: 16
                    visible: !QbzLocal.localAlbumsLoading && root.albumsView === "grid"
                        && root.albumsVisible.length > 0
                    rows: root.albumsVisible
                }
                ListView {
                    id: albumsList
                    anchors.fill: parent
                    anchors.leftMargin: 32
                    anchors.rightMargin: 32
                    anchors.topMargin: 12
                    visible: !QbzLocal.localAlbumsLoading && root.albumsView === "list"
                        && root.albumsVisible.length > 0
                    clip: true
                    cacheBuffer: 56 * 10
                    boundsBehavior: Flickable.StopAtBounds
                    model: root.albumsVisible
                    onContentYChanged: root.listReport(albumsList, root.albumsVisible, 56)
                    onModelChanged: root.listReport(albumsList, root.albumsVisible, 56)
                    Component.onCompleted: root.listReport(albumsList, root.albumsVisible, 56)
                    delegate: LocalAlbumRow {
                        required property var modelData
                        width: albumsList.width
                        item: modelData
                    }
                }
                QbzScrollBar {
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    target: albumsGrid
                    visible: albumsGrid.visible && albumsGrid.contentHeight > albumsGrid.height
                }
                QbzScrollBar {
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    target: albumsList
                    visible: albumsList.visible && albumsList.contentHeight > albumsList.height
                }
            }

            // ===================== ARTISTS TAB ========================
            // Two-column master/detail (LocalLibraryView.slint): the merged
            // artist rail on the left, that artist's albums on the right.
            Item {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "artists"
                    && root.albumDetail === null

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 36
                    visible: QbzLocal.localArtistsLoading
                }
                CenterNote {
                    visible: !QbzLocal.localArtistsLoading && root.artists.length === 0
                    text: QbzSession.tr("No artists in your local library yet.", QbzSession.trRev)
                }

                Row {
                    anchors.fill: parent
                    visible: !QbzLocal.localArtistsLoading && root.artists.length > 0
                    spacing: 0

                    Item {
                        width: 300
                        height: parent.height
                        ListView {
                            id: artistRail
                            anchors.fill: parent
                            anchors.leftMargin: 20
                            anchors.rightMargin: 8
                            anchors.topMargin: 12
                            clip: true
                            cacheBuffer: 56 * 10
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.artistsVisible
                            delegate: Rectangle {
                                required property var modelData
                                width: artistRail.width
                                height: 56
                                radius: 6
                                color: modelData.name === root.selectedArtist
                                    ? theme.surfaceElevated
                                    : arArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    anchors.rightMargin: 10
                                    spacing: 10
                                    Rectangle {
                                        width: 40
                                        height: 40
                                        radius: 20
                                        anchors.verticalCenter: parent.verticalCenter
                                        color: theme.surfaceElevated
                                        QbzIcon {
                                            name: "mic-vocal"
                                            width: 18
                                            height: 18
                                            anchors.centerIn: parent
                                            tintName: "muted"
                                        }
                                    }
                                    Column {
                                        width: parent.width - 60
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 2
                                        Text {
                                            width: parent.width
                                            text: modelData.name
                                            color: theme.textPrimary
                                            font.pixelSize: 13
                                            font.weight: theme.weightMedium
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: modelData.albumCount + " "
                                                + QbzSession.tr("albums", QbzSession.trRev)
                                                + "  •  " + modelData.trackCount + " "
                                                + QbzSession.tr("tracks", QbzSession.trRev)
                                            color: theme.textMuted
                                            font.pixelSize: 11
                                            elide: Text.ElideRight
                                        }
                                    }
                                }
                                MouseArea {
                                    id: arArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.selectedArtist = modelData.name
                                }
                            }
                        }
                        QbzScrollBar {
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            target: artistRail
                            visible: artistRail.contentHeight > artistRail.height
                        }
                    }
                    Rectangle {
                        width: 1
                        height: parent.height
                        color: theme.borderSubtle
                    }
                    Item {
                        width: parent.width - 301
                        height: parent.height
                        CenterNote {
                            visible: root.selectedArtist === ""
                            text: QbzSession.tr("Select an artist to see their albums.", QbzSession.trRev)
                        }
                        LocalAlbumGrid {
                            id: artistGrid
                            anchors.fill: parent
                            anchors.leftMargin: 24
                            anchors.rightMargin: 24
                            anchors.topMargin: 16
                            visible: root.selectedArtist !== ""
                            rows: root.artistAlbums
                        }
                        QbzScrollBar {
                            anchors.right: parent.right
                            anchors.rightMargin: 4
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            target: artistGrid
                            visible: artistGrid.visible
                                && artistGrid.contentHeight > artistGrid.height
                        }
                    }
                }
            }

            // ===================== FOLDERS TAB ========================
            Item {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "folders"
                    && root.albumDetail === null

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 36
                    visible: QbzLocal.localFoldersLoading
                }
                CenterNote {
                    visible: !QbzLocal.localFoldersLoading && root.folders.length === 0
                        && root.foldersSearch === ""
                    text: QbzSession.tr("No folders in your local library yet.", QbzSession.trRev)
                }

                // ---------------- FLAT MODE ----------------
                Item {
                    anchors.fill: parent
                    visible: !QbzLocal.localFoldersLoading && root.foldersMode === "flat"
                        && root.folders.length > 0
                    CenterNote {
                        visible: root.foldersVisible.length === 0
                        text: QbzSession.tr("No folders match your search.", QbzSession.trRev)
                    }
                    LocalAlbumGrid {
                        id: foldersGrid
                        anchors.fill: parent
                        anchors.leftMargin: 32
                        anchors.rightMargin: 32
                        anchors.topMargin: 16
                        visible: root.foldersGridView === "grid" && root.foldersVisible.length > 0
                        rows: root.foldersVisible
                    }
                    ListView {
                        id: foldersList
                        anchors.fill: parent
                        anchors.leftMargin: 32
                        anchors.rightMargin: 32
                        anchors.topMargin: 12
                        visible: root.foldersGridView === "list" && root.foldersVisible.length > 0
                        clip: true
                        cacheBuffer: 56 * 10
                        boundsBehavior: Flickable.StopAtBounds
                        model: root.foldersVisible
                        onContentYChanged: root.listReport(foldersList, root.foldersVisible, 56)
                        onModelChanged: root.listReport(foldersList, root.foldersVisible, 56)
                        delegate: LocalAlbumRow {
                            required property var modelData
                            width: foldersList.width
                            item: modelData
                        }
                    }
                    QbzScrollBar {
                        anchors.right: parent.right
                        anchors.rightMargin: 4
                        anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        target: foldersGrid
                        visible: foldersGrid.visible && foldersGrid.contentHeight > foldersGrid.height
                    }
                }

                // ---------------- TREE MODE ----------------
                Row {
                    anchors.fill: parent
                    visible: !QbzLocal.localFoldersLoading && root.foldersMode === "tree"
                        && root.folders.length > 0
                    spacing: 0

                    // ---- Left rail: header + the flattened tree ----
                    Item {
                        width: root.treeRailWidth
                        height: parent.height

                        Column {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 8
                            anchors.topMargin: 12
                            anchors.bottomMargin: 8
                            spacing: 8

                            Row {
                                width: parent.width
                                height: 30
                                spacing: 4
                                QbzIconButton {
                                    name: "chevron-up"
                                    btnSize: 30
                                    iconSize: 15
                                    onClicked: QbzLocal.treeCollapseAll()
                                }
                                LocalSearch {
                                    boxWidth: parent.width - 34
                                    placeholder: QbzSession.tr("Search folders", QbzSession.trRev)
                                    onEdited: function (v) {
                                        root.treeSearch = v
                                        treeSearchDebounce.restart()
                                    }
                                    Timer {
                                        id: treeSearchDebounce
                                        interval: 180
                                        onTriggered: QbzLocal.treeSearch(root.treeSearch)
                                    }
                                }
                            }

                            Item {
                                width: parent.width
                                height: parent.height - 38
                                QbzSpinner {
                                    anchors.centerIn: parent
                                    size: 28
                                    visible: QbzLocal.localTreeLoading
                                }
                                // The tree is a FLAT array — a windowing
                                // ListView, never a recursive component.
                                ListView {
                                    id: treeList
                                    anchors.fill: parent
                                    visible: !QbzLocal.localTreeLoading
                                    clip: true
                                    cacheBuffer: 26 * 20
                                    boundsBehavior: Flickable.StopAtBounds
                                    model: root.tree
                                    delegate: TreeRow {
                                        required property var modelData
                                        width: treeList.width
                                        node: modelData
                                        selected: modelData.path === root.selectedFolder
                                        onToggled: QbzLocal.treeToggle(modelData.path,
                                                                       !modelData.expanded)
                                        onActivated: {
                                            root.selectedFolder = modelData.path
                                            root.folderDetailSearch = ""
                                            QbzLocal.selectFolder(modelData.path)
                                        }
                                    }
                                }
                                QbzScrollBar {
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    anchors.bottom: parent.bottom
                                    target: treeList
                                    visible: treeList.contentHeight > treeList.height
                                }
                            }
                        }
                    }

                    // ---- Divider (the drag-to-resize handle is out of
                    //      scope — POC-NOTE; the 1px line is 1:1) ----
                    Rectangle {
                        width: 1
                        height: parent.height
                        color: theme.borderSubtle
                    }

                    // ---- Right pane: the selected folder's detail ----
                    Item {
                        width: parent.width - root.treeRailWidth - 1
                        height: parent.height

                        CenterNote {
                            visible: root.selectedFolder === ""
                            text: QbzSession.tr("Select a folder to see its contents.", QbzSession.trRev)
                        }
                        QbzSpinner {
                            anchors.centerIn: parent
                            size: 28
                            visible: QbzLocal.localDetailLoading
                        }

                        Flickable {
                            id: detailPane
                            anchors.fill: parent
                            visible: root.selectedFolder !== "" && !QbzLocal.localDetailLoading
                                && root.folderDetail !== null
                            clip: true
                            contentWidth: width
                            contentHeight: detailCol.height + 120
                            boundsBehavior: Flickable.StopAtBounds

                            Column {
                                id: detailCol
                                x: 28
                                y: 16
                                width: parent.width - 56
                                spacing: 14

                                // Header: circular Play + folder name +
                                // recursive count + search + grid/list.
                                Row {
                                    width: parent.width
                                    spacing: 12
                                    QbzCircleAction {
                                        name: "play-fill"
                                        primary: true
                                        anchors.verticalCenter: parent.verticalCenter
                                        onClicked: QbzLocal.playFolder(root.selectedFolder)
                                    }
                                    Column {
                                        width: parent.width - 40 - 200 - 60 - 3 * 12
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 2
                                        Text {
                                            width: parent.width
                                            text: root.folderDetail ? root.folderDetail.name : ""
                                            color: theme.textPrimary
                                            font.pixelSize: theme.fontSection
                                            font.weight: theme.weightSemibold
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: (root.folderDetail ? root.folderDetail.trackCount : 0)
                                                + " " + QbzSession.tr("tracks", QbzSession.trRev)
                                            color: theme.textMuted
                                            font.pixelSize: theme.fontLegal
                                            elide: Text.ElideRight
                                        }
                                    }
                                    LocalSearch {
                                        anchors.verticalCenter: parent.verticalCenter
                                        placeholder: QbzSession.tr("Search albums...", QbzSession.trRev)
                                        onEdited: function (v) { root.folderDetailSearch = v }
                                    }
                                    ViewToggle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        mode: root.folderDetailView
                                        onSetMode: function (v) { root.folderDetailView = v }
                                    }
                                }

                                // ---- Subfolders (cover grid or list) ----
                                Column {
                                    width: parent.width
                                    spacing: 12
                                    visible: root.folderDetail
                                        && root.folderDetail.subfolders.length > 0
                                    Text {
                                        text: QbzSession.tr("Folders", QbzSession.trRev)
                                        color: theme.textSecondary
                                        font.pixelSize: 12
                                        font.weight: theme.weightSemibold
                                        font.letterSpacing: 0.5
                                    }
                                    Text {
                                        visible: root.detailSubfolders.length === 0
                                        width: parent.width
                                        text: QbzSession.tr("No folders match your search.", QbzSession.trRev)
                                        color: theme.textMuted
                                        font.pixelSize: theme.fontBody
                                        horizontalAlignment: Text.AlignHCenter
                                    }
                                    // Wrapping cover grid — a Flow so the
                                    // outer Flickable owns the scrolling
                                    // (the card count per folder is small).
                                    Flow {
                                        visible: root.folderDetailView === "grid"
                                            && root.detailSubfolders.length > 0
                                        width: parent.width
                                        spacing: 12
                                        Repeater {
                                            model: root.detailSubfolders
                                            delegate: FolderSubcard {
                                                required property var modelData
                                                width: 140
                                                height: 182
                                                item: modelData
                                                onOpened: {
                                                    root.selectedFolder = modelData.path
                                                    root.folderDetailSearch = ""
                                                    QbzLocal.selectFolder(modelData.path)
                                                }
                                            }
                                        }
                                    }
                                    Column {
                                        visible: root.folderDetailView === "list"
                                            && root.detailSubfolders.length > 0
                                        width: parent.width
                                        spacing: 2
                                        Repeater {
                                            model: root.detailSubfolders
                                            delegate: Rectangle {
                                                required property var modelData
                                                width: parent.width
                                                height: 34
                                                radius: 6
                                                color: sfArea.containsMouse
                                                    ? theme.surfaceHover : "transparent"
                                                Row {
                                                    anchors.fill: parent
                                                    anchors.leftMargin: 8
                                                    anchors.rightMargin: 10
                                                    spacing: 8
                                                    QbzIcon {
                                                        name: "folder"
                                                        width: 14
                                                        height: 14
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        tintName: "muted"
                                                    }
                                                    Text {
                                                        width: parent.width - 60
                                                        height: parent.height
                                                        text: modelData.name
                                                        color: theme.textPrimary
                                                        font.pixelSize: 13
                                                        verticalAlignment: Text.AlignVCenter
                                                        elide: Text.ElideRight
                                                    }
                                                    Text {
                                                        height: parent.height
                                                        text: modelData.trackCount
                                                        color: theme.textMuted
                                                        font.pixelSize: 11
                                                        verticalAlignment: Text.AlignVCenter
                                                    }
                                                }
                                                MouseArea {
                                                    id: sfArea
                                                    anchors.fill: parent
                                                    hoverEnabled: true
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        root.selectedFolder = modelData.path
                                                        root.folderDetailSearch = ""
                                                        QbzLocal.selectFolder(modelData.path)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // ---- The folder's DIRECT tracks ----
                                Column {
                                    width: parent.width
                                    spacing: 6
                                    visible: root.folderDetail
                                        && root.folderDetail.tracks.length > 0
                                    Text {
                                        text: QbzSession.tr("Tracks", QbzSession.trRev)
                                        color: theme.textSecondary
                                        font.pixelSize: 12
                                        font.weight: theme.weightSemibold
                                        font.letterSpacing: 0.5
                                    }
                                    Repeater {
                                        model: root.folderDetail ? root.folderDetail.tracks : []
                                        delegate: LocalTrackRow {
                                            required property var modelData
                                            required property int index
                                            width: detailCol.width
                                            item: modelData
                                            number: modelData.number > 0
                                                ? modelData.number : index + 1
                                            onPlayRequested: QbzLocal.playFolderTrack(
                                                root.selectedFolder, modelData.id)
                                            onEnqueueRequested: function (m) {
                                                QbzLocal.enqueue("track", modelData.id, m)
                                            }
                                        }
                                    }
                                }

                                // Folder with neither tracks nor subfolders.
                                Text {
                                    visible: root.folderDetail
                                        && root.folderDetail.tracks.length === 0
                                        && root.folderDetail.subfolders.length === 0
                                    width: parent.width
                                    topPadding: 24
                                    text: QbzSession.tr("This folder has no tracks.", QbzSession.trRev)
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontBody
                                    horizontalAlignment: Text.AlignHCenter
                                }
                            }
                        }
                        QbzScrollBar {
                            anchors.right: parent.right
                            anchors.rightMargin: 4
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            target: detailPane
                            visible: detailPane.visible
                                && detailPane.contentHeight > detailPane.height
                        }
                    }
                }
            }

            // ===================== TRACKS TAB =========================
            // Server-paginated: 500 rows per page, appended as the list
            // approaches its tail. This is THE surface that froze the Slint
            // port at 16K tracks — the model is never the whole table and
            // the ListView only ever instantiates its window.
            Item {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "tracks"
                    && root.albumDetail === null

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 36
                    visible: QbzLocal.localTracksLoading
                }
                CenterNote {
                    visible: !QbzLocal.localTracksLoading && root.tracks.length === 0
                        && root.tracksSearch === ""
                    text: QbzSession.tr("No tracks in your local library yet.", QbzSession.trRev)
                }
                CenterNote {
                    visible: !QbzLocal.localTracksLoading && root.tracks.length === 0
                        && root.tracksSearch !== ""
                    text: QbzSession.tr("No tracks match your search.", QbzSession.trRev)
                }

                ListView {
                    id: tracksList
                    anchors.fill: parent
                    anchors.leftMargin: 32
                    anchors.rightMargin: 32
                    anchors.topMargin: 12
                    visible: !QbzLocal.localTracksLoading && root.tracks.length > 0
                    clip: true
                    cacheBuffer: 50 * 12
                    boundsBehavior: Flickable.StopAtBounds
                    model: root.tracks
                    onContentYChanged: {
                        root.listReport(tracksList, root.tracks, 50)
                        // Infinite scroll: 600px of runway, exactly the
                        // Slint's trigger distance.
                        if (contentY + height >= contentHeight - 600
                            && QbzLocal.localTracksHasMore
                            && !QbzLocal.localTracksLoadingMore
                            && !QbzLocal.localTracksLoading) {
                            QbzLocal.tracksLoadMore()
                        }
                    }
                    onModelChanged: root.listReport(tracksList, root.tracks, 50)
                    Component.onCompleted: root.listReport(tracksList, root.tracks, 50)

                    delegate: LocalTrackRow {
                        required property var modelData
                        required property int index
                        width: tracksList.width
                        item: modelData
                        number: index + 1
                        onPlayRequested: QbzLocal.playTrack(modelData.id)
                        onEnqueueRequested: function (m) {
                            QbzLocal.enqueue("track", modelData.id, m)
                        }
                    }

                    footer: Item {
                        width: tracksList.width
                        height: QbzLocal.localTracksLoadingMore ? 48 : 0
                        QbzSpinner {
                            anchors.centerIn: parent
                            size: 22
                            visible: QbzLocal.localTracksLoadingMore
                        }
                    }
                }
                QbzScrollBar {
                    anchors.right: parent.right
                    anchors.rightMargin: 4
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    target: tracksList
                    visible: tracksList.visible && tracksList.contentHeight > tracksList.height
                }
            }
        }
    }
}
