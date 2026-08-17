// Local Library — composition root for the QML port of
// crates/qbz-ui/ui/locallibrary/LocalLibraryView.slint (the shipping
// behaviour; that file is the ONLY reference).
//
// This file owns FOUR things and nothing else:
//   1. state — the Slint LocalLibraryState/LibAlbumFilterState fields;
//   2. the derived documents (search / sort / filter / grouping / A-Z), in
//      JS, because QbzLocal publishes ONE JSON document per surface (the
//      library_qt.rs transport rationale);
//   3. the fixed chrome (title row, tab bar, toolbar, divider) and which tab
//      body is mounted;
//   4. the ARTWORK WINDOW REGISTRY (`_windows` + `artMap`, below). This one
//      is here — and pushes the file past the 500-line guideline — because it
//      is the one policy that CANNOT be split per surface: several cover
//      surfaces are mounted at once (rail + grid, subfolders + track rows)
//      and eviction is only correct when it is decided against all of them
//      together. Splitting it per body is exactly the bug it replaced.
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
import QtQuick.Window
import com.blitzfc.qbz
import "../controls"
import "../theme"
import "local"

Rectangle {
    id: root

    // Transparent while the ambient background is active (the frosted
    // content panel shows through — AppShell's contentFrame owns the fill).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: theme.ambientOn
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
    // off | album | artist | name — PERSISTED, so it is owned by the bridge
    // exactly like the Tracks sort and the album-identity mode, not by a QML
    // default. It used to be a plain `property string tracksGroup: "off"`:
    // `locallibrary_ui.json` carried the user's choice across restarts and
    // nothing ever read it back into the UI (PARITY-DEBT #13).
    readonly property string tracksGroup: QbzLocal.localTracksGroup
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

    // Covers arrive ONE AT A TIME (src/local_artwork.rs phase 2 streams each
    // thumbnail through the id-keyed signal the moment it resolves). Rebinding
    // `artMap` per arrival is quadratic in the window: each arrival copied the
    // whole map and re-evaluated the art binding of EVERY mounted cell, so a
    // 50-cover window did ~2500 binding evaluations and 50 map copies during
    // exactly the seconds the grid is trying to stay smooth. Arrivals are
    // coalesced into ONE rebind per frame instead — the covers still appear
    // progressively (16ms granularity is invisible), at O(n) total cost.
    property var _artInbox: ({})
    Timer {
        id: artFlush
        interval: 16
        repeat: false
        onTriggered: {
            var m = Object.assign({}, root.artMap, root._artInbox)
            root._artInbox = ({})
            // A rebind needs a NEW object reference (same-ref assignment is
            // not a change in QML).
            root.artMap = m
        }
    }
    Connections {
        target: QbzLocal
        function onLocalArtworkReady(key, path) {
            root._artInbox[key] = path
            if (!artFlush.running) artFlush.start()
            // An arrival is live evidence that the pass is still running, so
            // it keeps every placeholder's settle countdown suspended. Without
            // this the bound is a fixed timeout from the REPORT, and a cold
            // local library resolves slower than that (see artPassMs).
            root.artPulse = true
            artPulseOff.restart()
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

    // A TAB route from the cortinilla's local "View more" links. Same shape
    // as consumePendingArtist above: the property change is the trigger, so
    // it must be released after it is applied or the same link cannot fire
    // twice in a row.
    function consumePendingRoute() {
        var raw = QbzLocal.localPendingRoute
        if (raw === "") return
        var route
        try { route = JSON.parse(raw) } catch (e) { QbzLocal.clearPendingRoute(); return }
        if (route.tab) activeTab = route.tab
        // The query pre-filters the TRACKS tab only; the albums and artists
        // tabs have no search box of their own.
        if (route.tab === "tracks" && route.query) {
            // Set the view's own search state too, not just the query, or the
            // box would render empty while the list is filtered.
            root.tracksSearch = route.query
            QbzLocal.tracksSearch(route.query)
        }
        QbzLocal.clearPendingRoute()
    }

    Connections {
        target: QbzLocal
        function onLocalPendingArtistChanged() { root.consumePendingArtist() }
        function onLocalPendingRouteChanged() { root.consumePendingRoute() }
    }

    // Mount: load the default tab. Tab switches load on demand (each tab is
    // one query; the Albums/Folders/Artists sets are bounded).
    Component.onCompleted: {
        QbzLocal.loadTab(root.activeTab)
        consumePendingArtist()
        consumePendingRoute()
    }
    onActiveTabChanged: {
        QbzLocal.loadTab(activeTab)
        // The Artists detail derives from the album set (the DB aggregates
        // the contributor list per album), so make sure it is loaded.
        if (activeTab === "artists" && albums.length === 0) QbzLocal.loadTab("albums")
        // The tab that just appeared has covers to ask for and the one that
        // left has covers to let go of. Each surface answers for itself.
        artworkRefresh()
    }

    /// "Re-report your window." Broadcast for the state changes no single
    /// surface can observe on its own (the tab switch). A surface that is
    /// hidden when it hears this releases its slot instead.
    signal artworkRefresh()

    // The folder-detail cover set is SMALL and fully mounted, so the whole
    // set is one window report (the grids/lists window themselves).
    //
    // BOTH of its cover sections contribute. The report used to carry the
    // subfolder cards only, while LocalFolderDetail ALSO binds `artMap` on
    // the folder's direct track rows — those rows asked for a key nobody had
    // requested, so their 36px cell stayed blank forever. The track half is
    // gated on `trackArtwork` (default OFF), which is why this is a derived
    // property: flipping the setting re-derives it and re-reports by itself.
    readonly property var folderDetailArtRows: {
        if (!folderDetail) return []
        var out = (folderDetail.subfolders || []).slice()
        if (trackArtwork && folderDetail.tracks) {
            for (var i = 0; i < folderDetail.tracks.length; i++)
                out.push(folderDetail.tracks[i])
        }
        return out
    }
    function reportFolderDetailWindow() {
        var rows = folderDetailArtRows
        if (rows.length === 0) { releaseWindow("folder-detail"); return }
        queueWindowReport(rows, 0, rows.length - 1, "folder-detail")
    }
    onFolderDetailArtRowsChanged: reportFolderDetailWindow()

    function reportEphemeralWindow() {
        var rows = (ephemeral && ephemeral.albums) ? ephemeral.albums : []
        if (rows.length === 0) { releaseWindow("ephemeral"); return }
        queueWindowReport(rows, 0, rows.length - 1, "ephemeral")
    }
    onEphemeralChanged: reportEphemeralWindow()

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
                // The chips are keyed off the FOLDED word, which is why
                // local_rows.rs keeps folding `qobuz_purchase` into "offline"
                // — a purchased album IS a Qobuz download as far as this row
                // of chips is concerned, and the badge reads `sourceRaw`
                // instead of splitting the bucket.
                //
                // The normalisation below is the reference's §10-H bug kept
                // closed: Tauri's three arms never mention `qobuz_purchase`,
                // so a raw word reaching here matches no chip and ticking ANY
                // source filter hides every purchased album, silently.
                var src = (r.source || "local").toLowerCase()
                if (src === "qobuz_purchase" || src === "qobuz_download") src = "offline"
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

    // Tracks: search + sort are SERVER-side (they define the pagination
    // order), so the loaded pages arrive already ordered and the visible set
    // is the pages as published.
    //
    // The GROUP modes are the exception: they are a CLIENT-side visual
    // reorder layered on top of that order (`derive_tracks`,
    // local_library.rs:1196-1247, sorts the rows by the group key before the
    // headers go in; commit e379aa65 calls it out as "group modes keep their
    // client-side visual reorder on top"). Doing it here rather than inside
    // LocalTracksTab keeps ONE definition of "the visible order" — the tab's
    // entry model, the artwork window, select-all and the play seam all read
    // this same array, which is what makes the queue match the screen
    // (PARITY-DEBT #14).
    function cmpStr(a, b) { return a < b ? -1 : a > b ? 1 : 0 }
    readonly property var tracksVisible: {
        var rows = tracks
        if (tracksGroup === "off" || rows.length === 0) return rows
        var lc = function (v) { return (v || "").toString().toLowerCase() }
        var out = rows.slice()
        if (tracksGroup === "album") {
            out.sort(function (a, b) {
                return cmpStr(lc(a.album), lc(b.album)) || cmpStr(lc(a.title), lc(b.title))
            })
        } else if (tracksGroup === "artist") {
            out.sort(function (a, b) {
                return cmpStr(lc(a.artist), lc(b.artist))
                    || cmpStr(lc(a.album), lc(b.album))
                    || cmpStr(lc(a.title), lc(b.title))
            })
        } else if (tracksGroup === "name") {
            out.sort(function (a, b) { return cmpStr(lc(a.title), lc(b.title)) })
        }
        return out
    }

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

    // The selected artist's albums.
    //
    // The MATCH lives in Rust — `QbzLocal.artistAlbumIds` ->
    // local_artist_match::album_matches_artist, the port of
    // local_library.rs:3368-3390 — because it is the same normalized-name rule
    // the Artists merge uses and there must be exactly one of it. It compares
    // NORMALIZED PARTS for exact equality against the primary artist, every
    // `allArtists` comma part and every split-credit part (`,&/;`, feat/ft/
    // featuring/with), plus the "various artists" special case.
    //
    // What it replaced: a lowercase equality on `artist` OR a SUBSTRING
    // `indexOf` on `allArtists`. That listed "Airbourne" and "Blair" under
    // "Air", and an album credited "A & B" never appeared under "B"
    // (PARITY-DEBT #8).
    //
    // The ids come back rather than the rows, so the pane renders the very
    // objects this view already parsed, in the published order.
    readonly property var artistAlbums: {
        if (selectedArtist === "") return []
        var rows = albums   // binding dependency: re-derive on every republish
        var ids = {}
        try {
            var arr = JSON.parse(QbzLocal.artistAlbumIds(selectedArtist))
            for (var j = 0; j < arr.length; j++) ids[arr[j]] = true
        } catch (e) {
            console.warn("[qbz-qt] local: bad artist album ids — " + e)
            return []
        }
        var out = []
        for (var i = 0; i < rows.length; i++) {
            if (ids[rows[i].id]) out.push(rows[i])
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
    // away. Same policy as LibraryView (the Slint eviction, QML-side), with
    // one correction the port needed: eviction is derived from EVERY live
    // surface at once, not from the one array the current call carries.
    //
    // WHY A REGISTRY AND NOT A SINGLE WINDOW. This view mounts TWO cover
    // surfaces side by side more often than not: the Artists tab is the
    // avatar rail plus that artist's album grid, and Folders tree mode is the
    // subfolder cards plus the folder's track rows. The old per-call eviction
    // built `keep` from its own rows, so each surface deleted the other's
    // covers on every report and both settled on grey squares. `_windows`
    // holds the last window of every live surface keyed by a stable surface
    // id, and the keep-set is their union.
    property var _windows: ({})

    /// Record (or drop) one surface's window. An empty or degenerate window
    /// RELEASES the slot rather than pinning covers the surface no longer
    /// shows.
    function applyWindow(key, rows, first, last) {
        if (!rows || rows.length === 0) { delete root._windows[key]; return }
        last = Math.min(last, rows.length - 1)
        first = Math.max(0, first)
        if (first > last) { delete root._windows[key]; return }
        root._windows[key] = { "rows": rows, "first": first, "last": last }
    }

    /// A surface that unmounts, or scrolls/tabs off screen, stops holding its
    /// covers. Cheap no-op when the surface was never registered.
    function releaseWindow(key) {
        var k = key || "default"
        if (root._windows[k] === undefined && root._pending[k] === undefined) return
        delete root._windows[k]
        delete root._pending[k]
        flushWindows()
    }

    /// Evict against the union of every live window, then request what is
    /// still missing. Keys already resolved are NOT re-sent: `artMap` IS the
    /// resolved set and a re-request costs Rust a `stat` per key
    /// (local_artwork.rs phase 1), which a scroll would pay per pass.
    function flushWindows() {
        var keep = {}
        var k, w, rows, i, ak, span, lo, hi
        for (k in root._windows) {
            w = root._windows[k]
            rows = w.rows
            span = w.last - w.first + 1
            lo = Math.max(0, w.first - span)
            hi = Math.min(rows.length - 1, w.last + span)
            for (i = lo; i <= hi; i++) {
                ak = rows[i] ? rows[i].artKey : ""
                if (ak) keep[ak] = true
            }
        }
        var m = root.artMap
        var changed = false
        for (k in m) if (!keep[k]) { delete m[k]; changed = true }
        // The not-yet-flushed arrivals are evicted too, or a cover that landed
        // for a row we have just scrolled away from would be re-added by the
        // next flush and defeat the eviction.
        for (k in root._artInbox) if (!keep[k]) delete root._artInbox[k]
        if (changed) root.artMap = Object.assign({}, m)

        var keys = []
        var seen = {}
        for (k in root._windows) {
            w = root._windows[k]
            rows = w.rows
            for (i = w.first; i <= w.last; i++) {
                ak = rows[i] ? rows[i].artKey : ""
                if (!ak || seen[ak]) continue
                if (m[ak] !== undefined || root._artInbox[ak] !== undefined) continue
                seen[ak] = true
                keys.push(ak)
            }
        }
        if (keys.length > 0) QbzLocal.artworkWindow(JSON.stringify(keys))
    }

    /// Immediate single-surface report — the leading edge and the rate-limit
    /// flush both land here.
    function reportWindow(rows, first, last, key) {
        applyWindow(key || "default", rows, first, last)
        flushWindows()
    }

    // ------------------------- skeleton pulse ----------------------------
    // ONE 900ms Timer drives EVERY placeholder in this view AND in every
    // tab body under views/local/ (they read `view.skelPhase`) — N
    // placeholders, 1 timer, which is QbzSkeleton's preferred drive mode.
    // GATING RULE: freeze on NOT VISIBLE — the view hidden, or the window
    // minimized/hidden. NEVER on lost focus (a tiling desktop keeps windows
    // visible and unfocused).
    property bool skelPhase: false
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    readonly property bool anyTabLoading: QbzLocal.localAlbumsLoading
        || QbzLocal.localArtistsLoading || QbzLocal.localFoldersLoading
        || QbzLocal.localTreeLoading || QbzLocal.localTracksLoading
        || QbzLocal.localTracksLoadingMore || QbzLocal.localDetailLoading
        || QbzLocal.localEphemeralLoading

    // THE CLEANUP RULE FOR LOCAL ARTWORK. src/local_artwork.rs
    // (`resolve_window_blocking`) DROPS keys with no cover — "the QML map
    // only ever grows with real hits" — so a local album with no embedded
    // art NEVER produces an artMap entry. A per-item placeholder gated only
    // on "artMap has no path yet" would therefore shimmer forever on every
    // artless album. `artSettleMs` is the bound: each placeholder fades out
    // by itself, revealing the card's own empty tile. Per-item handover
    // (QbzSkeleton.handedOver) still wins whenever the cover DOES arrive.
    //
    // The countdown does NOT start at mount — it starts when the artwork pass
    // goes quiet (`artPulse` below). A FIRST visit to a cold library decodes
    // every cover from scratch and takes seconds; a countdown from mount used
    // to expire a beat BEFORE the covers landed, which is the other half of
    // "the skeleton disappears before the art is rendered".
    readonly property int artSettleMs: 1200
    /// How long after the last sign of life a resolution pass is still
    /// presumed in flight. Re-armed by every window report AND by every cover
    /// that arrives, so a slow first visit extends it by itself instead of
    /// needing a pessimistic constant.
    readonly property int artPassMs: 2500
    // The pulse runs while an artwork window can still resolve, and is also
    // what holds off every placeholder's settle countdown.
    property bool artPulse: false
    Timer {
        id: artPulseOff
        interval: root.artPassMs
        onTriggered: root.artPulse = false
    }
    Timer {
        interval: 900
        repeat: true
        running: (root.anyTabLoading || root.artPulse)
            && root.visible && root.windowShowing
        onTriggered: root.skelPhase = !root.skelPhase
    }

    /// Does THIS row want a cover at all? An empty artKey means the row has
    /// no artwork slot and there is nothing to wait for.
    ///
    /// NOTE — this REPLACED `artPending(key)`, which asked "is the path still
    /// missing". That predicate is not the placeholder gate and never was:
    /// it goes false when the PATH lands, while the art needs a further load
    /// + canvas raster before it is on screen. The placeholder now hands over
    /// on QbzSkeleton's `coverSource`/`coverReady` (see QbzSkeleton.qml), and
    /// this function only answers "is a cover expected here".
    function artWanted(key) { return (key || "") !== "" }
    /// The decoded cover for a row, "" until it lands.
    function artPathOf(key) { return root.artMap[key] || "" }

    // Reporting is rate-limited to one pass per 180ms, but LEADING-EDGE: the
    // limiter exists to stop a scroll from firing a resolution pass per pixel,
    // and a view that has just mounted is not scrolling. Trailing-only cost
    // every first paint a flat 180ms before Rust was even asked for a cover —
    // pure latency on the one report the user is actually waiting for.
    //
    // TWO THINGS THIS USED TO SWALLOW, both of which read as "grey squares
    // until I scroll":
    //   - ONE pending slot for the whole view. Two surfaces reporting inside
    //     the same 180ms (the artist rail and the artist's album grid do
    //     exactly that on every selection) meant the second overwrote the
    //     first and one pane was never requested at all. Pending reports are
    //     keyed by surface now and ALL of them flush.
    //   - `restart()` on every call. That is a reset-on-change debounce: a
    //     continuous flick (or a burst of mount-time reports — rows arrive,
    //     the column count settles, the tab becomes visible) pushed the
    //     trailing edge out by another 180ms each time, so the pass fired
    //     only once everything stopped moving. It is a rate limiter now:
    //     `start()`, never `restart()`.
    Timer {
        id: windowDebounce
        interval: 180
        onTriggered: {
            root._lastReportMs = Date.now()
            var pending = root._pending
            root._pending = ({})
            var any = false
            for (var k in pending) {
                var w = pending[k]
                root.applyWindow(k, w.rows, w.first, w.last)
                any = true
            }
            if (any) root.flushWindows()
        }
    }
    /// Surface id -> the window waiting for the next flush.
    property var _pending: ({})
    // `real` is a double in QML — Date.now() (~1.7e12) needs the range.
    property real _lastReportMs: 0
    function queueWindowReport(rows, first, last, key) {
        var k = key || "default"
        if (!windowDebounce.running
            && Date.now() - root._lastReportMs >= windowDebounce.interval) {
            root._lastReportMs = Date.now()
            root.reportWindow(rows, first, last, k)
        } else {
            root._pending[k] = { "rows": rows, "first": first, "last": last }
            if (!windowDebounce.running) windowDebounce.start()
        }
        // Covers can now land: keep the shared pulse (and every placeholder's
        // settle hold) alive until artPassMs after the LAST sign of life —
        // this report, or the next cover that arrives.
        root.artPulse = true
        artPulseOff.restart()
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

    // --- Ctrl+A / Escape hotkeys interface (2026-08-03 hotkeys-port §4.6) --
    // The duck-typed seam the AppShell router calls. Only the albums and
    // tracks tabs have multi-select (the folder tree rail's select mode is
    // Rust-side with no select-all arm — not covered); on the artists /
    // folders tabs selectAll() is a deliberate no-op.
    readonly property bool multiSelectOn: root.albumsMultiSelect || root.tracksMultiSelect
    function selectAll() {
        if (root.activeTab === "albums") {
            if (!root.albumsMultiSelect) root.albumsMultiSelect = true
            root.albumsBulkAction("select-all")
        } else if (root.activeTab === "tracks") {
            if (!root.tracksMultiSelect) root.tracksMultiSelect = true
            root.tracksBulkAction("select-all")
        }
    }
    function exitMultiSelectMode() {
        // Same "leaving drops the selection" contract the toggle buttons
        // carry (toggleAlbumsMultiSelect / toggleTracksMultiSelect).
        if (root.albumsMultiSelect) { root.albumsMultiSelect = false; root.albumsSelected = ({}) }
        if (root.tracksMultiSelect) { root.tracksMultiSelect = false; root.tracksSelected = ({}) }
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

            // ONE TAB EXISTS AT A TIME. These four were sibling mounts gated
            // on `visible:`, which hides an item but does not stop QML from
            // building it — so every entry into Local Library instantiated all
            // four tab bodies and their views' first screenful of delegates,
            // to show one. The same divergence measured in HomeView.qml (see
            // the Home tab there for the numbers and for why `asynchronous` is
            // NOT the answer); the Tracks tab is the one that makes it matter
            // here, being the 16K-row view.
            //
            // `active` also carries the tab's own precondition: with no local
            // library there is nothing to build, and the empty state above is
            // what shows instead.
            Loader {
                anchors.fill: parent
                active: QbzLocal.localAvailable && root.activeTab === "albums"
                sourceComponent: LocalAlbumsTab { view: root }
            }
            Loader {
                anchors.fill: parent
                active: QbzLocal.localAvailable && root.activeTab === "artists"
                sourceComponent: LocalArtistsTab { view: root }
            }
            Loader {
                anchors.fill: parent
                active: QbzLocal.localAvailable && root.activeTab === "folders"
                sourceComponent: LocalFoldersTab { view: root }
            }
            Loader {
                anchors.fill: parent
                active: QbzLocal.localAvailable && root.activeTab === "tracks"
                sourceComponent: LocalTracksTab { view: root }
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
