// Local Library — composition root for the QML port of
// crates/qbz-ui/ui/locallibrary/LocalLibraryView.slint (the shipping
// behaviour; that file is the ONLY reference).
//
// This file owns THREE things and nothing else:
//   1. state — the Slint LocalLibraryState/LibAlbumFilterState fields;
//   2. the derived documents (search / sort / filter / grouping / A-Z), in
//      JS, because QbzLocal publishes ONE JSON document per surface (the
//      library_qt.rs transport rationale);
//   3. the fixed chrome (title row, tab bar, toolbar, divider) and which tab
//      body is mounted.
// Every body, rail, pane, popup and row lives in qml/views/local/.
//
// CHROME (identical to FavoritesView / LibraryView, per the Slint header):
//   row 1  "Local Library" title + settings gear + Plex sync
//   row 2  segmented tab bar (Albums / Artists / Folders / Tracks, with
//          count badges) on the left, the per-tab toolbar floated right
//   1px divider, then ONE content area.
//
// PERFORMANCE (this is the surface with the documented Slint freeze at 16K+
// tracks):
//   - Tracks is SERVER-paginated (500/page, appended on scroll);
//   - every list/grid is a windowing view (ListView over a chunk model),
//     never a Repeater over the full array — including the grouped arms and
//     the artist rail;
//   - the folder tree is a FLAT array windowed by a ListView, levels fetched
//     lazily on expand;
//   - artwork is id-keyed (artKey), requested only for the MOUNTED window,
//     resolved to a 256px thumbnail in Rust, and evicted from `artMap` once
//     it falls a full window away from the viewport.
//
// The local ALBUM detail is no longer inline: it is a routed page,
// views/LocalAlbumView.qml, exactly as the Slint routes to
// album/LocalAlbumView.slint.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"
import "local"

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
    property bool albumsMultiSelect: false
    property var albumsSelected: ({})
    readonly property int albumsSelectedCount: Object.keys(albumsSelected).length

    // Folders tab.
    property string foldersMode: "tree"     // flat | tree
    property string foldersSearch: ""
    property string foldersSort: "artist-asc"
    property string foldersGroup: "off"
    property string foldersGridView: "grid"
    property string treeSearch: ""
    property bool treeSelectMode: false
    property string selectedFolder: ""
    property string folderDetailSearch: ""
    property string folderDetailView: "grid"
    property int treeRailWidth: 272

    // Artists tab.
    property string artistsSearch: ""
    property string selectedArtist: ""

    // Tracks tab.
    property string tracksSearch: ""
    property string tracksGroup: "off"      // off | album | artist | name
    property bool tracksMultiSelect: false
    property var tracksSelected: ({})
    readonly property int tracksSelectedCount: Object.keys(tracksSelected).length

    // Albums quality/format/source filter (LibAlbumFilterState).
    property bool filterOpen: false
    property var filter: ({})
    readonly property int filterCount: {
        var n = 0
        for (var k in filter) if (filter[k]) n++
        return n
    }
    function toggleFilter(key) {
        var f = Object.assign({}, filter)
        f[key] = !f[key]
        if (!f[key]) delete f[key]
        filter = f
    }
    function clearFilter() { filter = ({}) }

    // Per-row artwork on the track lists — the Slint gates this on
    // AppearanceState.local-library-track-artwork for the freeze reason and
    // its default is OFF.
    readonly property bool trackArtwork: QbzLocal.localTrackArtwork

    readonly property bool ephemeralActive: QbzLocal.localEphemeralActive

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
    readonly property var ephemeral: parseDoc(QbzLocal.localEphemeralJson, null)

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

    // A name route from LocalAlbumView ("go to artist" on a local album:
    // local/Plex artists have no catalog id) lands the view on the Artists
    // tab with that artist selected.
    function consumePendingArtist() {
        var pending = QbzLocal.localPendingArtist
        if (pending === "") return
        activeTab = "artists"
        selectedArtist = pending
        QbzLocal.clearPendingArtist()
    }
    Connections {
        target: QbzLocal
        function onLocalPendingArtistChanged() { root.consumePendingArtist() }
    }

    // Mount: load the default tab. Tab switches load on demand (each tab is
    // one query; the Albums/Folders/Artists sets are bounded).
    Component.onCompleted: {
        QbzLocal.loadTab(root.activeTab)
        consumePendingArtist()
    }
    onActiveTabChanged: {
        QbzLocal.loadTab(activeTab)
        // The Artists detail derives from the album set (the DB aggregates
        // the contributor list per album), so make sure it is loaded.
        if (activeTab === "artists" && albums.length === 0) QbzLocal.loadTab("albums")
    }

    // The folder-detail cover set is SMALL and fully mounted, so the whole
    // set is one window report (the grids/lists window themselves).
    onFolderDetailChanged: {
        if (folderDetail) queueWindowReport(folderDetail.subfolders, 0,
                                            folderDetail.subfolders.length - 1)
    }
    onEphemeralChanged: {
        if (ephemeral && ephemeral.albums) {
            queueWindowReport(ephemeral.albums, 0, ephemeral.albums.length - 1)
        }
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

    // Quality / format / source chips. Each SECTION is an OR within itself
    // and an AND across sections — an empty section means "any".
    function applyFilter(rows) {
        if (filterCount === 0) return rows
        var qAny = filter.hires || filter.cd || filter.lossy
        var fAny = filter.flac || filter.alac || filter.ape || filter.wav
            || filter.mp3 || filter.aac || filter.other
        var sAny = filter.local || filter.offline || filter.plex
        var known = { "flac": 1, "alac": 1, "ape": 1, "wav": 1, "mp3": 1, "aac": 1 }
        var out = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            var tier = (r.qualityTier || "").toLowerCase()
            var fmt = (r.format || "").toLowerCase()
            if (qAny) {
                var qok = (filter.hires && (tier === "hires" || tier === "max"))
                    || (filter.cd && tier === "cd")
                    || (filter.lossy && (tier === "mp3" || tier === "lossy"))
                if (!qok) continue
            }
            if (fAny) {
                var fok = filter[fmt] === true
                    || (filter.other === true && known[fmt] !== 1)
                if (!fok) continue
            }
            if (sAny) {
                var src = (r.source || "local").toLowerCase()
                if (!filter[src]) continue
            }
            out.push(r)
        }
        return out
    }

    // A-Z / by-artist grouping — [{ letter, items }] plus the alpha jumps the
    // strips consume (the strip derives its own indices from the groups).
    function groupRows(rows, mode) {
        if (mode === "off") return []
        var buckets = {}
        var order = []
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            var key
            if (mode === "artist") {
                key = (r.artist || "").trim() || "—"
            } else {
                var c = ((r.title || "").trim().slice(0, 1) || "#").toUpperCase()
                key = (c >= "A" && c <= "Z") ? c : "#"
            }
            if (!buckets[key]) { buckets[key] = []; order.push(key) }
            buckets[key].push(r)
        }
        order.sort()
        var out = []
        for (i = 0; i < order.length; i++) {
            out.push({ "letter": order[i], "items": buckets[order[i]] })
        }
        return out
    }

    readonly property var albumsVisible:
        applyFilter(sortRows(filterRows(albums, albumsSearch), albumsSort))
    readonly property var albumsGrouped: groupRows(albumsVisible, albumsGroup)

    readonly property var foldersVisible:
        sortRows(filterRows(folders, foldersSearch), foldersSort)
    readonly property var foldersGrouped: groupRows(foldersVisible, foldersGroup)

    // Tracks: search + sort are SERVER-side (pagination order), so the
    // visible set is the loaded pages as published.
    readonly property var tracksVisible: tracks

    readonly property var artistsVisible: {
        var q = artistsSearch.trim().toLowerCase()
        if (q === "") return artists
        var out = []
        for (var i = 0; i < artists.length; i++) {
            if ((artists[i].name || "").toLowerCase().indexOf(q) >= 0) out.push(artists[i])
        }
        return out
    }
    readonly property int artistsVisibleCount: artistsVisible.length
    readonly property var artistsGrouped: {
        var rows = artistsVisible
        var buckets = {}
        var order = []
        for (var i = 0; i < rows.length; i++) {
            var c = ((rows[i].name || "").trim().slice(0, 1) || "#").toUpperCase()
            var key = (c >= "A" && c <= "Z") ? c : "#"
            if (!buckets[key]) { buckets[key] = []; order.push(key) }
            buckets[key].push(rows[i])
        }
        order.sort()
        var out = []
        for (i = 0; i < order.length; i++) {
            out.push({ "letter": order[i], "items": buckets[order[i]] })
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

    // ============================ actions ================================
    function openAlbum(id) {
        QbzLocal.openAlbum(id)
        QbzShell.navigateTo("localalbum")
    }
    function selectFolder(path) {
        selectedFolder = path
        folderDetailSearch = ""
        QbzLocal.selectFolder(path)
    }
    function selectArtist(name) { selectedArtist = name }

    function toggleAlbumsMultiSelect() {
        albumsMultiSelect = !albumsMultiSelect
        if (!albumsMultiSelect) albumsSelected = ({})
    }
    function toggleAlbumSelected(id) {
        var s = Object.assign({}, albumsSelected)
        if (s[id]) delete s[id]; else s[id] = true
        albumsSelected = s
    }
    function albumsBulkAction(action) {
        if (action === "clear") { albumsSelected = ({}); return }
        if (action === "select-all") {
            var s = {}
            for (var i = 0; i < albumsVisible.length; i++) s[albumsVisible[i].id] = true
            albumsSelected = s
            return
        }
        QbzLocal.bulkAction("album", JSON.stringify(Object.keys(albumsSelected)), action)
    }

    function toggleTracksMultiSelect() {
        tracksMultiSelect = !tracksMultiSelect
        if (!tracksMultiSelect) tracksSelected = ({})
    }
    function toggleTrackSelected(id) {
        var s = Object.assign({}, tracksSelected)
        if (s[id]) delete s[id]; else s[id] = true
        tracksSelected = s
    }
    function tracksBulkAction(action) {
        if (action === "clear") { tracksSelected = ({}); return }
        if (action === "select-all") {
            var s = {}
            for (var i = 0; i < tracksVisible.length; i++) s[tracksVisible[i].id] = true
            tracksSelected = s
            return
        }
        QbzLocal.bulkAction("track", JSON.stringify(Object.keys(tracksSelected)), action)
    }

    function toggleTreeSelectMode() {
        treeSelectMode = !treeSelectMode
        QbzLocal.treeSetSelectMode(treeSelectMode)
    }

    // ============================ view ===================================

    Column {
        anchors.fill: parent
        spacing: 0

        // ---- Fixed chrome: title row, tab row, divider (91px) -----------
        LocalChrome {
            width: parent.width
            view: root
        }

        // ===================== CONTENT =================================
        Item {
            id: contentArea
            width: parent.width
            height: parent.height - 91
            clip: true

            // "Nothing indexed yet" (no db / no registered folder).
            QbzEmptyState {
                visible: !QbzLocal.localAvailable
                anchors.centerIn: parent
                iconName: "folder-plus"
                title: QbzSession.tr("No local library yet", QbzSession.trRev)
                body: QbzSession.tr("Add a music folder to scan your local files.", QbzSession.trRev)
                actionLabel: QbzSession.tr("Open Local Library settings", QbzSession.trRev)
                onActionClicked: QbzShell.navigateTo("settings")
            }

            LocalAlbumsTab {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "albums"
                view: root
            }
            LocalArtistsTab {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "artists"
                view: root
            }
            LocalFoldersTab {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "folders"
                view: root
            }
            LocalTracksTab {
                anchors.fill: parent
                visible: QbzLocal.localAvailable && root.activeTab === "tracks"
                view: root
            }
        }
    }

    // Albums filter popup — sibling of the content column so it FLOATS over
    // it instead of taking a layout slot (the Slint mount, :2470).
    LocalFilterPopup {
        anchors.fill: parent
        visible: root.filterOpen && root.activeTab === "albums"
        view: root
    }
}
