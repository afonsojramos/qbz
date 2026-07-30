// Library view — QML port of crates/qbz-ui/ui/favorites/FavoritesView.slint
// + the Library "All" mixed feed (library_all.rs semantics).
//
// Data: QbzLibrary.libraryJson (ONE JSON document — the full merged feed;
// tabs/search/sort/source-filters derive HERE in JS, measured per the
// phase brief) + libraryCountsJson (tab badges). Artwork is id-keyed
// through the libraryArtworkReady signal into `artMap` (never a
// wrong-cover race); windows are reported as artKeys via
// libraryArtworkWindow, and artMap entries far outside the viewport are
// pruned (the Slint eviction policy, QML-side).
//
// POC-NOTEs:
// - Multi-select, group modes, alpha jumps, play-all / shuffle bulk
//  actions, playlists-random, artists sidepanel, albums LIST mode, per-tab
//  persist of view/sort state: out of scope (stubs or omitted; the All
//  grid+list is the measured 1:1 focus).
// - Filter by genre IS wired, on the All toolbar only — that is the single
//  place the Slint FavoritesView draws it (FavoritesView.slint:864,
//  `context: "library-all"`). It filters CLIENT-side over the feed, exactly
//  like library_all.rs::derive: an item shows when its (lowercased) genre
//  contains one of the selected genre names (+ their sub-genres), so rows
//  with no genre at all (artist / label / playlist) drop out while a genre
//  is selected. The selection lives in the shared per-context store and
//  persists to genre_filter.json.
// - Offline: the generic OfflinePlaceholder replica mounts (the Slint
//  offline RAIL of playable cached favorites needs the offline cache —
//  not wired).
// - Sort/search/source state is session-local (no ui_prefs persistence).

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../rows"
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

    // ============================ state ==================================
    property string activeTab: "all"

    readonly property var counts: JSON.parse(QbzLibrary.libraryCountsJson)
    // Full merged feed (parsed once per publish — timed for the report).
    readonly property var feed: parseFeed(QbzLibrary.libraryJson)
    function parseFeed(json) {
        var t = Date.now()
        var f = JSON.parse(json)
        console.log("[qbz-qt][perf] QML JSON.parse feed: " + (Date.now() - t)
                    + "ms (" + f.length + " items, " + json.length + " bytes)")
        return f
    }

    // Decoded-cover map {artKey: file://path}, fed by the signal.
    property var artMap: ({})

    // --- skeleton pulse ---------------------------------------------------
    // ONE 900ms Timer drives EVERY placeholder in this view (the loading
    // grid/list AND the per-item cover tiles). GATING RULE: freeze on NOT
    // VISIBLE — the view hidden, or the window minimized/hidden. NEVER on
    // lost focus (a tiling desktop keeps windows visible and unfocused).
    property bool skelPhase: false
    // Covers requested for the current window that have not arrived yet, and
    // the window's first index (the per-item placeholders count their cap
    // from it, so the cap follows the viewport instead of the model).
    property int artPending: 0
    property int artWindowFirst: 0
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    Timer {
        interval: 900
        repeat: true
        // Ticks only while something can actually be shimmering.
        running: root.visible && root.windowShowing
            && (QbzLibrary.libraryLoading || root.artPending > 0)
        onTriggered: root.skelPhase = !root.skelPhase
    }

    // All-tab derive state (LibraryAllState semantics).
    property string search: ""
    property string sortBy: "date"      // "date" | "title" | "artist"
    property bool sortAsc: false        // date: false = newest first
    property bool showPurchases: true
    property bool showFavorites: true
    property bool showFollowing: true
    property bool showLocal: true
    property string viewMode: "grid"    // "grid" | "list"

    // --- shared genre filter, "library-all" context ----------------------
    // Read STRAIGHT off the bridge singleton, not off libGenrePopup: the
    // popup is declared LAST (z-order) and a creation-time binding that
    // dereferences a not-yet-created id registers NO dependency, so it would
    // never re-evaluate. The popup instance is only touched from click
    // handlers, which run long after creation.
    readonly property var genreDoc: {
        try {
            return JSON.parse(QbzBridge.genreFilterJson)
        } catch (e) {
            return {}
        }
    }
    readonly property int genreCount: (genreDoc.counts || {})["library-all"] || 0
    // Selected genre NAMES (+ sub-genres), lowercased once here — the port's
    // FeedItem keeps the display casing (the Slint model lowercases at build
    // time instead, library_all.rs:540). Reading this inside visibleItems()
    // is what makes the grid/list model re-derive when a chip is toggled.
    readonly property var genreNames: {
        var src = (genreDoc.names || {})["library-all"] || []
        var out = []
        for (var i = 0; i < src.length; i++)
            out.push(String(src[i]).toLowerCase())
        return out
    }

    // Other-tab state.
    property string tabSearch: ""
    property string albumsSort: "default" // default|title-asc|title-desc|artist-asc
    property string playlistsSubTab: "favorites"

    // ONE shared per-tab query, reset on entry — that IS the reference.
    // In Slint every entry into one of the five tabs runs
    // navigate_favorites -> load_favorites -> apply_favorites, which assigns
    // "" to that tab's search (favorites.rs:570/595/614/627/649) AND, because
    // the ExpandableSearch lives inside an `if` branch, destroys and
    // re-creates it COLLAPSED. So a tab is never re-entered with a stale
    // filter and five independent queries would be wrong. The port has one
    // feed and no per-tab reload, so the reset is explicit here.
    // The All tab is deliberately NOT reset: LibraryAllState.search is never
    // cleared anywhere in the reference, so its feed stays filtered behind a
    // collapsed magnifier — quirk included.
    onActiveTabChanged: {
        tabSearch = ""
        if (tabSearchBox) tabSearchBox.closeSearch()
    }

    // Per-tab emptiness gate. Slint gates each per-tab toolbar (search AND
    // genre/sort/sub-tab/view toggles) on the tab's FULL loaded set
    // (FavoritesView.slint:593/636/707/722/747) — searching down to zero
    // results must NOT hide it, so this counts the feed and never
    // visibleItems(). Same keep-rules as visibleItems() minus the playlists
    // sub-tab split, which makes the playlists count the favorites-OR-
    // following the reference gate wants (counts.playlists is favorites
    // only, library_qt.rs:479).
    readonly property var tabTotals: {
        var c = { "tracks": 0, "albums": 0, "artists": 0, "playlists": 0, "labels": 0 }
        for (var i = 0; i < feed.length; i++) {
            var it = feed[i]
            if (it.kind === "track") { if (it.group === "favorites") c.tracks++ }
            else if (it.kind === "album") { if (it.group === "favorites") c.albums++ }
            else if (it.kind === "artist") c.artists++
            else if (it.kind === "label") c.labels++
            else if (it.kind === "playlist") c.playlists++
        }
        return c
    }
    readonly property bool tabHasItems:
        activeTab === "all" || (tabTotals[activeTab] || 0) > 0

    // ------------------------- derive (JS) ------------------------------
    // THE model, derived once and held here. Both views bind to it and both
    // window reports read IT — never `grid.model` / `list.model` back off
    // the view, which is a hard crash, not a style point:
    //
    //   QQuickItemView::model() (libQt6Quick 6.11.1, +0x145dfe..+0x145e2c)
    //   reads THREE things off the private and guards only two of them:
    //     0x588  the QPointer's ExternalRefCountData guard  -> null-checked,
    //            and its strongref at +4 is checked too
    //     0x7e0  bit 0, the "this view owns a delegate model" flag
    //     0x590  the QPointer's raw VALUE -> handed to
    //            QQmlDelegateModel::model() with NO test at all
    //   So the getter is safe against a QPointer whose guard says "gone" and
    //   unsafe against the window where the guard still says "alive" while
    //   the value slot no longer refers to a usable object. setModel() opens
    //   exactly that window: it tears the old QQmlDelegateModel down, and its
    //   own repositioning emits contentYChanged from inside the teardown,
    //   straight into onContentYChanged and straight back into the getter.
    //   `this` arrives null (gdb: rsi=0), +8 is the first member read, SIGSEGV
    //   at address 8 in QQmlDelegateModel::model()+5. Reproduced standalone
    //   under qml6 6.11.1 (same fault ip, same dmesg Code bytes); it is the
    //   same fault the app took on 2026-07-27/28/29 going Library -> Labels
    //   with the grid scrolled — the tab switch re-derives the model AND the
    //   clamp to the shorter list moves contentY. With contentY already 0
    //   nothing is emitted and nothing crashes, which is why it was
    //   intermittent rather than every switch.
    //
    //   PRECONDITION, precisely, because the wrong one invites the wrong fix:
    //   the old delegate model is being DESTROYED, not momentarily unset. It
    //   does not come back a moment later. So the repair for any site with
    //   this shape is NOT "defer the readback one event-loop turn" (Qt.
    //   callLater / a 0ms Timer) — by then there is still no old model and,
    //   worse, the handler has been decoupled from the state it was meant to
    //   report. The only repair is to not ask the view for its model.
    //
    //   onModelChanged is NOT the entry: it fires after the swap is complete
    //   and the same readback there survives. Bisected, not assumed.
    //
    //   Only `.model` is unguarded. `count`, `contentHeight`, `currentIndex`,
    //   `indexAt()` and `itemAtIndex()` all null-check and are safe in the
    //   same handler — which is why the LocalLibrary tabs, which cache their
    //   array in `entries` and only ever ask the view for indices, never had
    //   this bug. Same shape here.
    //
    // Deriving once also replaces two independent passes over the feed (the
    // grid and the list each used to call visibleItems() on every change)
    // with one, and guarantees both views and the debounce Timer see the
    // SAME array instance.
    // Named `visibleRows`, not `visible` — root is a Rectangle and `visible`
    // is taken.
    readonly property var visibleRows: visibleItems()

    function visibleItems() {
        var items = []
        var i
        if (activeTab === "all") {
            var needle = search.toLowerCase()
            var anyGroup = showPurchases || showFavorites || showFollowing
            // Genre gate (library_all.rs::derive): empty = no filter;
            // otherwise the item's genre must contain one of the selected
            // names, so genre-less kinds are excluded.
            var genres = root.genreNames
            for (i = 0; i < feed.length; i++) {
                var it = feed[i]
                var isLocal = it.source === "local" || it.source === "plex"
                if (isLocal) {
                    if (!showLocal) continue
                } else if (anyGroup) {
                    var g = it.group
                    var ok = (g === "purchases" && showPurchases)
                        || (g === "favorites" && showFavorites)
                        || (g === "following" && showFollowing)
                    if (!ok) continue
                }
                if (needle !== ""
                    && it.title.toLowerCase().indexOf(needle) < 0
                    && it.artist.toLowerCase().indexOf(needle) < 0) continue
                if (genres.length > 0) {
                    var itemGenre = (it.genre || "").toLowerCase()
                    if (itemGenre === "") continue
                    var genreHit = false
                    for (var gi = 0; gi < genres.length; gi++) {
                        if (itemGenre.indexOf(genres[gi]) >= 0) { genreHit = true; break }
                    }
                    if (!genreHit) continue
                }
                items.push(it)
            }
            // Canonical ascending order per field, then reverse for the
            // other direction; "date" keeps model order (newest-first),
            // reversed for oldest (library_all.rs derive).
            if (sortBy === "title") {
                items.sort(function (a, b) { return a.title.toLowerCase() < b.title.toLowerCase() ? -1 : 1 })
                if (!sortAsc) items.reverse()
            } else if (sortBy === "artist") {
                items.sort(function (a, b) { return a.artist.toLowerCase() < b.artist.toLowerCase() ? -1 : 1 })
                if (!sortAsc) items.reverse()
            } else if (sortAsc) {
                items = items.slice().reverse()
            }
            return items
        }
        var tabNeedle = tabSearch.toLowerCase()
        function hit(it) {
            return tabNeedle === ""
                || it.title.toLowerCase().indexOf(tabNeedle) >= 0
                || it.artist.toLowerCase().indexOf(tabNeedle) >= 0
        }
        for (i = 0; i < feed.length; i++) {
            var x = feed[i]
            var keep = false
            if (activeTab === "tracks") keep = x.kind === "track" && x.group === "favorites"
            else if (activeTab === "albums") keep = x.kind === "album" && x.group === "favorites"
            else if (activeTab === "artists") keep = x.kind === "artist"
            else if (activeTab === "labels") keep = x.kind === "label"
            else if (activeTab === "playlists") keep = x.kind === "playlist"
                && (playlistsSubTab === "following" ? x.group === "following" : x.group === "favorites")
            if (keep && hit(x)) items.push(x)
        }
        if (activeTab === "albums" && albumsSort !== "default") {
            var field = albumsSort.indexOf("artist") === 0 ? "artist" : "title"
            items.sort(function (a, b) {
                var av = field === "artist" ? a.artist.toLowerCase() : a.title.toLowerCase()
                var bv = field === "artist" ? b.artist.toLowerCase() : b.title.toLowerCase()
                return av < bv ? -1 : 1
            })
            if (albumsSort.slice(-4) === "desc") items.reverse()
        }
        return items
    }

    // --------------------- windowed artwork -----------------------------
    // Report the mounted window as artKeys; prune far-away covers.
    function reportWindow(visibleArray, first, last) {
        if (visibleArray.length === 0) return
        last = Math.min(last, visibleArray.length - 1)
        if (first > last) return
        var keys = []
        var keep = {}
        var span = last - first + 1
        var keepLo = Math.max(0, first - span)
        var keepHi = Math.min(visibleArray.length - 1, last + span)
        for (var i = keepLo; i <= keepHi; i++) keep[visibleArray[i].artKey] = true
        var m = artMap
        var pending = 0
        for (i = first; i <= last; i++) {
            var k = visibleArray[i].artKey
            if (visibleArray[i].imageUrl !== "") {
                keys.push(k)
                if (!m[k]) pending++
            }
        }
        // Drives the skeleton Timer + the per-item cap (see the pulse block).
        artWindowFirst = first
        artPending = pending
        var changed = false
        for (var key in m) {
            if (!keep[key]) { delete m[key]; changed = true }
        }
        if (changed) artMap = Object.assign({}, m)
        QbzLibrary.libraryArtworkWindow(JSON.stringify(keys))
    }

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.artMap
            // One fewer shimmering tile — when the count hits 0 the pulse
            // Timer stops on its own.
            if (!m[key] && root.artPending > 0) root.artPending--
            m[key] = path
            // Rebind requires a NEW object reference (same-ref assignment
            // is not a change in QML).
            root.artMap = Object.assign({}, m)
        }
        // Favourite state settled in the store. Patched IN PLACE, and
        // `feedChanged()` is deliberately NOT called — that is the WHOLE fix
        // for "clicking a heart in Library > All throws me back to the top".
        //
        //   feedChanged() -> `visibleRows` re-derives -> a NEW array reaches
        //   `model:` -> QQuickItemView::setModel() -> the view resets its
        //   scroll offset to 0 and rebuilds every delegate.
        //
        // Scrolled to row 40 in a 3,000-row feed, one heart click sent the
        // user back to row 0. Nothing on screen needed the re-derive: the
        // grid cards each listen to this same signal for their own key
        // (cards/AlbumCard.qml, TrackCard.qml, PlaylistCard.qml,
        // ArtistCard.qml) and the list rows do the same (FeedListRow below).
        // The in-place mutation is for the delegates built LATER — when the
        // user scrolls, re-filters or switches tab — and it reaches them
        // because `visibleRows` pushes the very same row objects.
        //
        // Same reasoning as onPinChanged below; the two are now symmetric.
        function onLibraryFavoriteChanged(key, value) {
            var f = root.feed
            for (var i = 0; i < f.length; i++) {
                if (f[i].artKey === key) { f[i].isFavorite = value; break }
            }
        }
        // Pin state settled in the store. The row is patched IN PLACE and the
        // feed is deliberately NOT re-signalled: `feedChanged()` re-derives
        // `visibleRows`, which swaps the GridView/ListView model and tears
        // down every delegate — per pin click. The glyphs on screen do not
        // need it, because each card listens to this same signal for its own
        // key (cards/AlbumCard.qml); the mutation here is for the delegates
        // built LATER, when the user scrolls or re-filters. `visibleRows`
        // pushes the very same row objects, so both see the new value.
        function onPinChanged(key, value) {
            var f = root.feed
            for (var i = 0; i < f.length; i++) {
                if (f[i].artKey === key) { f[i].isPinned = value; break }
            }
        }
    }

    // Debounced window reporting (180ms, library_all.rs throttle).
    Timer {
        id: windowDebounce
        interval: 180
        onTriggered: root.reportWindow(root.visibleRows, pendingFirst, pendingLast)
        property int pendingFirst: 0
        property int pendingLast: 0
    }
    function queueWindowReport(first, last) {
        windowDebounce.pendingFirst = first
        windowDebounce.pendingLast = last
        windowDebounce.restart()
    }

    // All-tab search settle -> re-report the first window so the filtered
    // head gets covers. Lives at ROOT scope (its callee already does) so it
    // survives the search control it used to be parented to.
    Timer {
        id: allSearchDebounce
        interval: 250
        onTriggered: root.queueWindowReport(0, 59)
    }

    // ============================ components =============================

    // Segmented tab (SegmentedTabBar's Segment) with count badge.
    // Small toolbar toggle button (ToggleButton sm): 30px, active = accent.
    component ToolToggle: Rectangle {
        property string name: ""
        property bool active: false
        signal clicked()
        width: 30
        height: 30
        radius: 6
        color: active ? theme.surfaceElevated
             : ttArea.containsMouse ? theme.surfaceHover : "transparent"
        QbzIcon {
            name: parent.name
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: parent.active ? "accent"
                   : ttArea.containsMouse ? "textPrimary" : "secondary"
        }
        MouseArea {
            id: ttArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Filter-by-genre trigger (FavoritesView.slint's FavGenreButton, sm):
    // accent fill + "N genres" while the "library-all" selection is active.
    component GenreToolButton: Rectangle {
        id: gtb
        readonly property bool active: root.genreCount > 0
        width: gtbRow.width
        height: 30
        radius: 6
        color: gtb.active ? theme.accent
             : gtbArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
        Row {
            id: gtbRow
            height: parent.height
            leftPadding: 10
            rightPadding: 12
            spacing: 7
            QbzIcon {
                name: "list-filter"
                width: 13
                height: 13
                anchors.verticalCenter: parent.verticalCenter
                // Same pair-of-halves fix as controls/BrowseGenreButton.qml:
                // the glyph said "primary" (legacy alias of a literal
                // #ffffff) next to an accent-text label, i.e. a white glyph
                // beside a black label on the pale-accent themes.
                // favorites/FavoritesView.slint:301 + :309 are consistently
                // #ffffff, but that white is 1.70:1 on high-contrast, 1.74
                // on ikari, 1.82 on wcag-dark and under 2.6:1 on 16 of the
                // 35 palettes — deliberate divergence from both lines.
                // theme/QbzTheme.qml, "ON AN ACCENT FILL".
                tintName: gtb.active ? theme.accentGlyphTint : "secondary"
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.genreCount === 0
                    ? QbzSession.tr("Filter by genre", QbzSession.trRev)
                    : root.genreCount === 1
                        ? QbzSession.tr("1 genre", QbzSession.trRev)
                        : QbzSession.tr("{} genres", QbzSession.trRev)
                            .replace("{}", root.genreCount)
                // The colour twin of the glyph's tint above — accent-text on
                // 34 of the 35 palettes.
                color: gtb.active ? theme.accentGlyphColor : theme.textSecondary
                font.pixelSize: 12
            }
        }
        MouseArea {
            id: gtbArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: libGenrePopup.toggle()
        }
    }

    // (The inline toolbar SearchBox is gone — both toolbars now mount the
    // shared controls/QbzLineEdit in its expandable arm, which IS
    // primitives/ExpandableSearch.slint 1:1.)

    // Card heart (live favorite toggle).


    // Quality mini-badge (hi-res image / cd box).
    // ===================== card families (Slint homologation) =============
    // Phase 13/21: every Library surface mounts the SAME card family the
    // Slint mounts (FavoritesView.slint): track -> discover/TrackCard,
    // album -> discover/AlbumCard, artist -> discover/ArtistGridCard,
    // playlist -> discover/PlaylistCard, label -> discover/LabelCard — the
    // shared qml/ files (TrackCard/AlbumCard/ArtistCard/PlaylistCard/
    // LabelCard.qml). The old one-size FeedGridCard is gone.

    // ------------------- per-row play (PARITY-DEBT #5) -------------------
    // Every track row in this view queues the list the user is LOOKING AT,
    // anchored on the clicked row — it does NOT play one track and stop.
    //
    // Slint: playback.rs:3518-3524
    //   order_by_visible(FavoritesState.tracks-visible,   // rendered order
    //                    favorites::play_tracks(),        // FAV_CURRENT
    //                    clicked_id) -> play_tracks(tracks, idx)
    // and, when order_by_visible returns None (the clicked row does not
    // resolve inside the visible list), the single-track fallback at :3620.
    //
    // The visible ORDER is the whole point: it carries the tab, the search,
    // the sort and the genre/source filters. In this port that order exists
    // only here (the feed is derived in JS), so it goes down as a JSON id
    // array and `library_qt::order_by_visible` resolves it against the feed.
    // `visibleRows` is THE derived model both views bind to — never read
    // `grid.model` / `list.model` back off a view (see the note on it).
    function visibleTrackIds() {
        var rows = root.visibleRows
        var ids = []
        for (var i = 0; i < rows.length; i++)
            if (rows[i].kind === "track") ids.push(rows[i].id)
        return ids
    }
    function playTrackInContext(trackId) {
        // Feature-detect the seam so the view degrades to the old one-track
        // play instead of throwing if the invokable is not there yet.
        if (typeof QbzLibrary.libraryPlayVisible === "function") {
            QbzLibrary.libraryPlayVisible(JSON.stringify(visibleTrackIds()), trackId)
            return
        }
        console.warn("[qbz-qt] LibraryView: QbzLibrary.libraryPlayVisible is missing;"
                     + " falling back to a one-track queue (PARITY-DEBT #5)")
        QbzPlayer.playTrack(trackId)
    }

    // Track context-menu model (TrackCard.slint track-menu) + dispatch —
    // shared by the LIST rows here (the grid card carries its own copy in
    // TrackCard.qml).
    // `favorite` is passed in rather than read off `item`: the row owns the
    // live state (see FeedListRow.favorite), `item.isFavorite` is only its
    // seed.
    function trackMenuModel(item, favorite) {
        var m = [
            { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
            { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
            { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
            { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
        ]
        if (item.artistId !== "") m.push({ "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" })
        if (item.albumId !== "") m.push({ "label": QbzSession.tr("Go to album", QbzSession.trRev), "icon": "disc", "action": "go-album" })
        m.push({ "label": favorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                 "icon": favorite ? "heart-filled" : "heart", "action": "favorite" })
        return m
    }
    // Takes the ROW, not the item — "favorite" has to go through the row's
    // own property (a write to `item.isFavorite` notifies nothing).
    function trackAction(row, a) {
        var item = row.item
        if (a === "play") root.playTrackInContext(item.id)
        else if (a === "next") QbzPlayer.enqueueTrack(item.id, "next")
        else if (a === "later") QbzPlayer.enqueueTrack(item.id, "later")
        else if (a === "queue") QbzPlayer.enqueueTrack(item.id, "queue")
        else if (a === "go-artist") QbzArtist.openArtist(item.artistId)
        else if (a === "go-album") QbzAlbum.openAlbum(item.albumId)
        else if (a === "favorite") row.toggleFavorite()
    }

    // --- All-feed LIST row (FavoritesView inline row, homologated) --------
    component FeedListRow: Rectangle {
        id: feedRow
        property var item: ({})
        // Viewport-relative position, for the skeleton's animated cap.
        property int rowIndex: 0
        height: 44
        radius: 6
        color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

        // Live heart, as a real property — `item.isFavorite = !…` notified
        // nothing (plain JS object; see rows/TrackRow.qml for the measurement)
        // so this row's glyph and its menu label never moved on click. The
        // binding is re-established on every new row object, so a republished
        // feed still wins.
        property bool favorite: feedRow.item.isFavorite === true
        onItemChanged: feedRow.favorite = Qt.binding(function () {
            return feedRow.item.isFavorite === true
        })
        function toggleFavorite() {
            feedRow.favorite = !feedRow.favorite
            QbzLibrary.libraryToggleFavorite(feedRow.item.kind, feedRow.item.id)
        }
        // Settle + rollback + cross-surface walk. `artKey` IS
        // `library_qt::feed_key(kind, id)`, the very key the signal carries —
        // which is also why root's own handler can patch the backing row by
        // comparing against it.
        Connections {
            target: QbzLibrary
            function onLibraryFavoriteChanged(key, value) {
                if ((feedRow.item.artKey || "") !== "" && key === feedRow.item.artKey)
                    feedRow.favorite = value
            }
        }

        // Row body — click plays/opens by kind. Declared BEFORE the cells so
        // the ⋯ button and art-play win their clicks.
        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (item.kind === "track") root.playTrackInContext(item.id)
                else if (item.kind === "album") QbzAlbum.openAlbum(item.id)
                else if (item.kind === "artist") QbzArtist.openArtist(item.id)
                // playlist/label pages: out of scope (POC-NOTE).
            }
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 12
            // Col — artwork (round for artists) + hover play.
            Rectangle {
                width: 44
                height: 44
                anchors.verticalCenter: parent.verticalCenter
                radius: item.kind === "artist" ? 22 : 6
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    visible: !lrCollage.visible
                    source: root.artMap[item.artKey] || ""
                    radius: item.kind === "artist" ? 22 : 6
                    // A label logo is contain-fitted (cropping cuts the
                    // wordmark) on a FLAT surface — those are transparent
                    // PNGs whose edges carry no colour to derive from.
                    // A playlist takes "auto": its own graphic is the 800x380
                    // `image_rectangle` and pads with an image-derived
                    // gradient, while a square member cover still crops. The
                    // flag is not required for that any more, but it stays
                    // authoritative where it IS published (this feed).
                    fit: item.kind === "label" ? "contain"
                        : item.kind === "playlist"
                            ? (item.playlistOwnImage === true ? "pad" : "auto")
                            : "crop"
                }
                // User playlists have no graphic of their own, so they show the
                // member-cover mosaic instead of a blank tile.
                PlaylistCollage {
                    id: lrCollage
                    anchors.fill: parent
                    visible: item.kind === "playlist"
                        && item.playlistOwnImage !== true
                        && (root.artMap[item.artKey] || "") === ""
                    urls: item.covers || []
                    radius: 6
                }
                // Per-row cover placeholder — same progressive rule as the
                // grid: it clears when THIS row's cover lands. Skipped for
                // the collage arm (a playlist mosaic is real content) and
                // for artists/labels (round designed placeholders).
                QbzSkeleton {
                    variant: "art"
                    anchors.fill: parent
                    blockRadius: 6
                    visible: (item.kind === "track" || item.kind === "album")
                        && item.imageUrl !== ""
                        && (root.artMap[item.artKey] || "") === ""
                    phase: root.skelPhase
                    cellIndex: rowIndex
                }
                Rectangle {
                    visible: item.kind === "track" || item.kind === "album" || item.kind === "playlist"
                    anchors.fill: parent
                    radius: item.kind === "artist" ? 22 : 6
                    color: "#a6000000"
                    opacity: lrPlayArea.containsMouse ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                    // On the #a6000000 artwork scrim — dark under every theme.
                    QbzIcon { name: "play-fill"; width: 16; height: 16; anchors.centerIn: parent; tintName: "white" }
                    MouseArea {
                        id: lrPlayArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (item.kind === "track") root.playTrackInContext(item.id)
                            else if (item.kind === "album") QbzPlayer.playAlbum(item.id)
                            // playlist play: out of scope (POC-NOTE).
                        }
                    }
                }
            }
            // Col — title + subtitle (artist link for track/album).
            Column {
                width: parent.width - 44 - 116 - (srcCol.visible ? 44 : 0)
                    - 150 - 18 - 30 - 6 * 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 18
                    text: item.title
                    color: lrTitleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: 14
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: lrTitleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (item.kind === "track") root.playTrackInContext(item.id)
                            else if (item.kind === "album") QbzAlbum.openAlbum(item.id)
                            else if (item.kind === "artist") QbzArtist.openArtist(item.id)
                        }
                    }
                }
                Text {
                    visible: item.subtitle !== ""
                    width: parent.width
                    height: 16
                    text: item.subtitle
                    color: (item.artistId !== "" && (item.kind === "track" || item.kind === "album")
                            && lrSubArea.containsMouse) ? theme.accent : theme.textMuted
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: lrSubArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: (item.artistId !== "" && (item.kind === "track" || item.kind === "album"))
                            ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (item.artistId !== "") QbzArtist.openArtist(item.artistId)
                    }
                }
            }
            // Col — type (icon + caps label).
            Rectangle {
                width: 116
                height: parent.height
                color: "transparent"
                Row {
                    spacing: 6
                    anchors.verticalCenter: parent.verticalCenter
                    QbzIcon {
                        name: item.kind === "track" ? "music"
                            : item.kind === "playlist" ? "list-music"
                            : item.kind === "artist" ? "user"
                            : item.kind === "label" ? "disc-3" : "disc"
                        width: 13
                        height: 13
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "muted"
                    }
                    Text {
                        text: item.kind === "track" ? QbzSession.tr("Track", QbzSession.trRev)
                            : item.kind === "album" ? QbzSession.tr("Album", QbzSession.trRev)
                            : item.kind === "artist" ? QbzSession.tr("Artist", QbzSession.trRev)
                            : item.kind === "playlist" ? QbzSession.tr("Playlist", QbzSession.trRev)
                            : QbzSession.tr("Label", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 1.2
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
            // Col — source glyph (only when local/Plex can appear).
            Rectangle {
                id: srcCol
                visible: root.showLocal
                width: 44
                height: parent.height
                color: "transparent"
                QbzIcon {
                    visible: item.source === "local" || item.source === "plex"
                    name: "hard-drive"
                    width: 14
                    height: 14
                    anchors.verticalCenter: parent.verticalCenter
                    // Host is a transparent column over the theme row (NOT an
                    // artwork scrim), so this matches its siblings
                    // local/LocalAlbumRow.qml and local/LocalTrackRow.qml.
                    tintName: item.source === "plex" ? "accent" : "muted"
                }
            }
            // Col — quality (albums + tracks).
            Rectangle {
                width: 150
                height: parent.height
                color: "transparent"
                QualityMini {
                    visible: (item.kind === "album" || item.kind === "track") && item.qualityTier !== ""
                    tier: item.qualityTier
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            // Col — favorite / follow indicator.
            QbzIcon {
                name: item.kind === "artist" ? "user-plus"
                    : (feedRow.favorite ? "heart-filled" : "heart")
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                tintName: "accent"
            }
            // Col — context-menu button.
            Rectangle {
                width: 30
                height: 30
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: lrMenuArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { name: "ellipsis"; width: 16; height: 16; anchors.centerIn: parent; tintName: "secondary" }
                MouseArea {
                    id: lrMenuArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { lrMenu.openAtCursor(lrMenuArea, mouse.x, mouse.y) }
                }
            }
        }
        CardMenu {
            id: lrMenu
            menuWidth: 196
            entries: item.kind === "track" ? root.trackMenuModel(item, feedRow.favorite)
                : item.kind === "album" ? [
                    { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
                    { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                    { "label": feedRow.favorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                      "icon": feedRow.favorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
                : item.kind === "artist" ? [
                    { "label": QbzSession.tr("Go to artist", QbzSession.trRev), "icon": "user", "action": "go-artist" },
                ]
                : [
                    { "label": QbzSession.tr("Open", QbzSession.trRev), "icon": "list-music", "action": "open" },
                    { "label": feedRow.favorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev),
                      "icon": feedRow.favorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
            onPicked: function (a) {
                if (item.kind === "track") { root.trackAction(feedRow, a); return }
                if (a === "open") {
                    if (item.kind === "album") QbzAlbum.openAlbum(item.id)
                    // playlist/label pages: out of scope (POC-NOTE).
                } else if (a === "play") {
                    if (item.kind === "album") QbzPlayer.playAlbum(item.id)
                } else if (a === "go-artist") {
                    QbzArtist.openArtist(item.id)
                } else if (a === "favorite") {
                    feedRow.toggleFavorite()
                }
            }
        }
    }

    // --- Tracks-tab row (primitives/TrackRow.slint, 50px) -----------------
    // ============================ view ===================================

    // Offline gate (OfflinePlaceholder replica; the Slint offline RAIL is
    // out of scope — see header note).
    QbzOfflinePlaceholder {
        visible: QbzSession.offline
        anchors.centerIn: parent
        showSettingsAction: true
        onSettingsClicked: QbzShell.navigateTo("settings")
    }

    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzSession.offline

        // --- Toolbar: tab menu (left) + per-tab controls (right) ---------
        Item {
            width: parent.width
            height: 56

            // FavTabMenu (SegmentedTabBar with count badges).
            Rectangle {
                x: 32
                y: 25 - height / 2
                width: tabRow.width
                height: tabRow.height
                color: theme.surfaceElevated
                radius: 6
                Row {
                    id: tabRow
                    padding: 3
                    spacing: 4
                    QbzTabBar {
                        counts: true
                        underline: true
                        activeId: root.activeTab
                        tabs: [
                            { "id": "all", "label": QbzSession.tr("All", QbzSession.trRev), "count": counts.all || 0 },
                            { "id": "tracks", "label": QbzSession.tr("Tracks", QbzSession.trRev), "count": counts.tracks || 0 },
                            { "id": "albums", "label": QbzSession.tr("Albums", QbzSession.trRev), "count": counts.albums || 0 },
                            { "id": "artists", "label": QbzSession.tr("Artists", QbzSession.trRev), "count": counts.artists || 0 },
                            { "id": "playlists", "label": QbzSession.tr("Playlists", QbzSession.trRev), "count": counts.playlists || 0 },
                            { "id": "labels", "label": QbzSession.tr("Labels", QbzSession.trRev), "count": counts.labels || 0 },
                        ]
                        onSelected: function (id) { root.activeTab = id }
                    }
                }
            }

            // Per-tab controls.
            Row {
                x: parent.width - width - 32
                y: 25 - height / 2
                height: 30
                spacing: 8

                // ===== All toolbar =====
                Row {
                    visible: root.activeTab === "all"
                    spacing: 8
                    height: parent.height
                    // FavoritesView.slint:801-813 — collapsed magnifier that
                    // opens LEFT over the tab bar; the All tab is the only
                    // one that debounces after the keystroke.
                    QbzLineEdit {
                        searchMode: true
                        expandable: true
                        sm: true
                        elevated: false          // ExpandableSearch fill = surface-card
                        openWidth: 196           // = max-open-width
                        placeholder: QbzSession.tr("Search your library", QbzSession.trRev)
                        onEdited: function (v) {
                            root.search = v
                            allSearchDebounce.restart()
                        }
                    }
                    // Source switches (tooltips are the Slint copies).
                    ToolToggle { name: "shopping-bag"; active: root.showPurchases; onClicked: root.showPurchases = !root.showPurchases }
                    ToolToggle { name: "heart"; active: root.showFavorites; onClicked: root.showFavorites = !root.showFavorites }
                    ToolToggle { name: "user-plus"; active: root.showFollowing; onClicked: root.showFollowing = !root.showFollowing }
                    ToolToggle { name: "hard-drive"; active: root.showLocal; onClicked: root.showLocal = !root.showLocal }
                    // Filter by genre — shared popup, own "library-all"
                    // context (FavoritesView.slint:864).
                    GenreToolButton { }
                    // Grid / list toggle.
                    ToolToggle {
                        name: root.viewMode === "list" ? "layout-grid" : "list"
                        active: false
                        onClicked: root.viewMode = root.viewMode === "list" ? "grid" : "list"
                    }
                    // Sort menu (PlaylistView-style: field + direction caret).
                    Rectangle {
                        width: allSortRow.width
                        height: 30
                        radius: 6
                        color: allSortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                        Row {
                            id: allSortRow
                            height: parent.height
                            leftPadding: 10
                            rightPadding: 10
                            spacing: 6
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzSession.tr("Sort", QbzSession.trRev) + ": " + (
                                    root.sortBy === "title" ? QbzSession.tr("Title", QbzSession.trRev)
                                    : root.sortBy === "artist" ? QbzSession.tr("Artist", QbzSession.trRev)
                                    : QbzSession.tr("Date added", QbzSession.trRev))
                                color: theme.textSecondary
                                font.pixelSize: 12
                            }
                            QbzIcon {
                                name: root.sortAsc ? "chevron-up" : "chevron-down"
                                width: 12
                                height: 12
                                anchors.verticalCenter: parent.verticalCenter
                                tintName: "secondary"
                            }
                        }
                        MouseArea {
                            id: allSortArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: allSortMenu.openBelowRight(allSortArea)
                        }
                        QbzContextMenu {
                            id: allSortMenu
                            menuWidth: 172
                                Repeater {
                                    model: [
                                        { "field": "date", "label": QbzSession.tr("Date added", QbzSession.trRev) },
                                        { "field": "title", "label": QbzSession.tr("Title", QbzSession.trRev) },
                                        { "field": "artist", "label": QbzSession.tr("Artist", QbzSession.trRev) },
                                    ]
                                    delegate: Rectangle {
                                        required property var modelData
                                        width: parent ? parent.width : 0
                                        height: 33
                                        radius: 5
                                        color: sortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                        Row {
                                            anchors.fill: parent
                                            anchors.leftMargin: 8
                                            spacing: 6
                                            Text {
                                                width: parent.width - 26
                                                height: parent.height
                                                text: modelData.label
                                                color: theme.textSecondary
                                                font.pixelSize: 13
                                                font.weight: root.sortBy === modelData.field ? theme.weightSemibold : theme.weightRegular
                                                verticalAlignment: Text.AlignVCenter
                                            }
                                            QbzIcon {
                                                visible: root.sortBy === modelData.field
                                                name: root.sortAsc ? "chevron-up" : "chevron-down"
                                                width: 12
                                                height: 12
                                                anchors.verticalCenter: parent.verticalCenter
                                                tintName: "accent"
                                            }
                                        }
                                        MouseArea {
                                            id: sortOptArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                // Re-pick flips direction; a
                                                // new field resets to its
                                                // natural default.
                                                if (root.sortBy === modelData.field) {
                                                    root.sortAsc = !root.sortAsc
                                                } else {
                                                    root.sortBy = modelData.field
                                                    root.sortAsc = modelData.field !== "date"
                                                }
                                                allSortMenu.close()
                                            }
                                        }
                                    }
                                }
                        }
                    }
                }

                // ===== Other-tab toolbars =====
                // ONE instance for the five tabs: the reference clears the
                // query on every tab entry (see onActiveTabChanged), so
                // per-tab instances would only implement a persistence that
                // does not exist. A Row skips `visible: false` children, so
                // the 30px slot appears/disappears cleanly and, because the
                // ROOT width never leaves collapsedSize while the field
                // opens, nothing to its right ever moves.
                QbzLineEdit {
                    id: tabSearchBox
                    visible: root.activeTab !== "all" && root.tabHasItems
                    searchMode: true
                    expandable: true
                    sm: true
                    elevated: false
                    openWidth: 196
                    placeholder: QbzSession.tr("Search", QbzSession.trRev)
                    onEdited: function (v) { root.tabSearch = v }
                }
                // Albums sort popup (real, JS-side).
                Rectangle {
                    visible: root.activeTab === "albums" && root.tabHasItems
                    width: albumsSortRow.width
                    height: 30
                    radius: 6
                    color: albumsSortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    Row {
                        id: albumsSortRow
                        height: parent.height
                        leftPadding: 10
                        rightPadding: 10
                        spacing: 6
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: QbzSession.tr("Sort", QbzSession.trRev) + ": " + root.albumsSort
                            color: theme.textSecondary
                            font.pixelSize: 12
                        }
                        QbzIcon { name: "chevron-down"; width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    }
                    MouseArea {
                        id: albumsSortArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: albumsSortMenu.openBelowRight(albumsSortArea)
                    }
                    QbzContextMenu {
                        id: albumsSortMenu
                        menuWidth: 172
                            Repeater {
                                model: ["default", "title-asc", "title-desc", "artist-asc"]
                                delegate: Rectangle {
                                    required property string modelData
                                    width: parent ? parent.width : 0
                                    height: 33
                                    radius: 5
                                    color: abSortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                    Text {
                                        anchors.fill: parent
                                        anchors.leftMargin: 8
                                        text: modelData
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        font.weight: root.albumsSort === modelData ? theme.weightSemibold : theme.weightRegular
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    MouseArea {
                                        id: abSortOptArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: { root.albumsSort = modelData; albumsSortMenu.close() }
                                    }
                                }
                            }
                    }
                }
                // Playlists sub-tab (Library / Following).
                Rectangle {
                    visible: root.activeTab === "playlists" && root.tabHasItems
                    width: plSubRow.width
                    height: 30
                    radius: 6
                    color: theme.surfaceElevated
                    Row {
                        id: plSubRow
                        padding: 3
                        spacing: 4
                        Repeater {
                            model: [
                                { "id": "favorites", "label": QbzSession.tr("Library", QbzSession.trRev) },
                                { "id": "following", "label": QbzSession.tr("Following", QbzSession.trRev) },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                property bool active: root.playlistsSubTab === modelData.id
                                width: plSubText.implicitWidth + 20
                                height: 24
                                radius: 4
                                color: active ? theme.surfaceMain : "transparent"
                                Text {
                                    id: plSubText
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: parent.active ? theme.textPrimary : theme.textMuted
                                    font.pixelSize: 12
                                    font.weight: theme.weightMedium
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.playlistsSubTab = modelData.id
                                }
                            }
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
            width: parent.width
            height: parent.height - 57
            clip: true

            // Loading skeleton — a deliberate ADDITION: the Slint mounts a
            // bare centred 36px LoadingSpinner here (FavoritesView.slint:
            // 956/1012), which says "busy" but not "this is the shape of
            // what is coming". The placeholders take the exact footprint of
            // the cells that will replace them (200x246 grid cells / 44-50px
            // rows), so the first paint reads as the page filling in.
            //
            // COST: the layer only exists while `libraryLoading` — the
            // Repeater models collapse to 0 otherwise, so nothing is mounted
            // and nothing animates once the feed lands. The mounted count is
            // capped to the viewport (never the model), and QbzSkeleton caps
            // ANIMATED instances at 48; the pulse itself is one shared bool.
            Item {
                id: skelLayer
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 16
                visible: QbzLibrary.libraryLoading && QbzLibrary.libraryError === ""
                // Which shape the tab is about to render.
                readonly property bool listShape:
                    (root.activeTab === "all" && root.viewMode === "list")
                    || root.activeTab === "tracks"
                readonly property int cols: Math.max(1, Math.floor(width / 220))
                readonly property int rows: Math.max(1, Math.ceil(height / 266))
                readonly property int rowHeight: root.activeTab === "tracks" ? 50 : 44

                Grid {
                    columns: skelLayer.cols
                    columnSpacing: 20
                    rowSpacing: 20
                    Repeater {
                        model: (skelLayer.visible && !skelLayer.listShape)
                            ? Math.min(48, skelLayer.cols * skelLayer.rows) : 0
                        delegate: QbzSkeleton {
                            required property int index
                            variant: "card"
                            // The real cell exactly: GridView cellWidth 220 /
                            // cellHeight 266 = 200x246 + the 20px gaps.
                            width: 200
                            height: 246
                            phase: root.skelPhase
                            cellIndex: index
                        }
                    }
                }
                Column {
                    width: parent.width
                    spacing: 6
                    Repeater {
                        model: (skelLayer.visible && skelLayer.listShape)
                            ? Math.min(48, Math.ceil(skelLayer.height / (skelLayer.rowHeight + 6))) : 0
                        delegate: QbzSkeleton {
                            required property int index
                            variant: "row"
                            width: skelLayer.width
                            height: skelLayer.rowHeight
                            phase: root.skelPhase
                            cellIndex: index
                        }
                    }
                }
            }

            // Error + retry.
            Column {
                visible: !QbzLibrary.libraryLoading && QbzLibrary.libraryError !== ""
                anchors.centerIn: parent
                spacing: 10
                Text {
                    text: QbzSession.tr("Couldn't load your Library.", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightSemibold
                }
                Text {
                    text: QbzLibrary.libraryError
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
                Rectangle {
                    width: retryText.implicitWidth + 28
                    height: 32
                    radius: 6
                    anchors.horizontalCenter: parent.horizontalCenter
                    color: retryArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle
                    Text {
                        id: retryText
                        anchors.centerIn: parent
                        text: QbzSession.tr("Retry", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontLegal
                    }
                    MouseArea {
                        id: retryArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzLibrary.reloadLibrary()
                    }
                }
            }

            // ============ GRID (all tabs; All in grid mode) ==============
            GridView {
                id: grid
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 16
                visible: !QbzLibrary.libraryLoading && QbzLibrary.libraryError === ""
                    && (root.activeTab !== "all" || root.viewMode === "grid")
                    && root.activeTab !== "tracks"
                cellWidth: 220
                cellHeight: 266
                cacheBuffer: 266 * 2
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleRows

                onContentYChanged: root.gridWindowReport()
                onModelChanged: root.gridWindowReport()
                onWidthChanged: root.gridWindowReport()
                Component.onCompleted: root.gridWindowReport()

                delegate: Item {
                    required property var modelData
                    required property int index
                    width: 200
                    height: 246

                    Component {
                        id: albumCardComp
                        AlbumCard {
                            albumId: modelData.id
                            title: modelData.title
                            artist: modelData.artist
                            artistId: modelData.artistId
                            genre: modelData.genre
                            year: modelData.year
                            qualityTier: modelData.qualityTier
                            artSource: root.artMap[modelData.artKey] || ""
                            isFavorite: modelData.isFavorite
                            isPinned: modelData.isPinned
                            // The pin payload's display snapshot: the REMOTE
                            // url, never `artMap` (a local file:// cache path
                            // that means nothing after a cache wipe). Without
                            // it every album pinned from the Library grid
                            // landed in the Home Pinned rail as a permanent
                            // grey placeholder, across restarts.
                            artworkUrl: modelData.imageUrl || ""
                            source: modelData.source
                        }
                    }
                    Component {
                        id: trackCardComp
                        TrackCard {
                            item: modelData
                            artSource: root.artMap[modelData.artKey] || ""
                            showSourceBadge: root.showLocal
                        }
                    }
                    Component {
                        id: artistCardComp
                        ArtistCard {
                            item: modelData
                            artSource: root.artMap[modelData.artKey] || ""
                            isPinned: modelData.isPinned === true
                            // Snapshot url for the pin payload (see the album
                            // card above).
                            artworkUrl: modelData.imageUrl || ""
                            // The Slint reference SPLITS here, and this one
                            // component serves both of its call sites: the
                            // ARTISTS tab grids pass follow-mode "none"
                            // (FavoritesView.slint:1820/1854 — every artist in
                            // that grid is followed by definition, so the chip
                            // would be a permanent tick), while the mixed ALL
                            // feed keeps the default and seeds `following`
                            // from `is-favorite` (:1143-1147), which is
                            // exactly the second spelling ArtistCard reads.
                            followMode: root.activeTab === "artists" ? "none" : "toggle"
                        }
                    }
                    Component {
                        id: playlistCardComp
                        PlaylistCard {
                            // artworkUrl is not passed on purpose: the card
                            // defaults it to `item.imageUrl`, which IS this
                            // row's remote cover (FeedItem.image_url).
                            item: modelData
                            artSource: root.artMap[modelData.artKey] || ""
                            isPinned: modelData.isPinned === true
                        }
                    }
                    Component {
                        id: labelCardComp
                        LabelCard {
                            item: modelData
                            artSource: root.artMap[modelData.artKey] || ""
                        }
                    }
                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData.kind === "album" ? albumCardComp
                            : modelData.kind === "track" ? trackCardComp
                            : modelData.kind === "artist" ? artistCardComp
                            : modelData.kind === "playlist" ? playlistCardComp
                            : labelCardComp
                    }
                    // THE fix for "many grey squares that fill in unevenly":
                    // each pending cover shimmers on its own and stops the
                    // moment ITS file:// path lands in artMap, so the grid
                    // resolves progressively instead of looking like a wall
                    // of dead tiles. Artists and labels are excluded — their
                    // cards already draw a designed round gradient+glyph
                    // portrait placeholder (ArtistGridCard/LabelCard).
                    // A bare Rectangle: it does not take pointer events, so
                    // the card's hover/click areas keep working underneath.
                    QbzSkeleton {
                        variant: "art"
                        width: 200
                        height: 200
                        visible: (modelData.kind === "album"
                                  || modelData.kind === "track"
                                  || modelData.kind === "playlist")
                            && modelData.imageUrl !== ""
                            && (root.artMap[modelData.artKey] || "") === ""
                        phase: root.skelPhase
                        // Viewport-relative so the 48-instance cap follows
                        // the window, not the (possibly 10k-item) model.
                        cellIndex: Math.max(0, index - root.artWindowFirst)
                    }
                }
            }

            // ============ LIST (All list mode + Tracks tab) ==============
            ListView {
                id: list
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 10
                visible: !QbzLibrary.libraryLoading && QbzLibrary.libraryError === ""
                    && ((root.activeTab === "all" && root.viewMode === "list")
                        || root.activeTab === "tracks")
                cacheBuffer: 44 * 10
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleRows
                onContentYChanged: root.listWindowReport()
                onModelChanged: root.listWindowReport()
                Component.onCompleted: root.listWindowReport()

                delegate: Loader {
                    required property var modelData
                    required property int index
                    width: list.width
                    height: root.activeTab === "tracks" ? 50 : 44
                    Component {
                        id: trackRowComp
                        TrackRow {
                            item: modelData
                            number: index + 1
                            onPlayRequested: root.playTrackInContext(item.id)
                            onEnqueueRequested: function (m) { QbzPlayer.enqueueTrack(item.id, m) }
                            // MyQBZ "Add to mixtape" — the HOST builds the
                            // AddItem array (TrackRow does not know
                            // itemType/source).
                            onMixtapeRequested: QbzMyQbzAdd.open(JSON.stringify([{
                                "itemType": "track", "source": "qobuz",
                                "sourceItemId": item.id, "title": item.title || "",
                                "subtitle": item.artist || "", "artworkUrl": "",
                                "year": null, "trackCount": null
                            }]))
                        }
                    }
                    Component {
                        id: feedRowComp
                        FeedListRow {
                            item: modelData
                            rowIndex: Math.max(0, index - root.artWindowFirst)
                        }
                    }
                    sourceComponent: root.activeTab === "tracks" ? trackRowComp : feedRowComp
                }
            }

            // Thin auto-hiding scrollbars (ListScrollbar replica).
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: grid
                visible: grid.visible && grid.contentHeight > grid.height
            }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: list
                visible: list.visible && list.contentHeight > list.height
            }
        }
    }

    // Both run from onContentYChanged, i.e. from INSIDE QQuickItemView::
    // setModel() — see the visibleRows comment. Everything they touch on the
    // view (width/height/contentY/cellWidth/cellHeight) is a plain qreal on
    // the private; the array comes from root, never from the view.
    function gridWindowReport() {
        var cols = Math.max(1, Math.floor(grid.width / grid.cellWidth))
        var firstRow = Math.max(0, Math.floor(grid.contentY / grid.cellHeight) - 1)
        var lastRow = Math.ceil((grid.contentY + grid.height) / grid.cellHeight) + 1
        var m = root.visibleRows
        queueWindowReport(firstRow * cols, Math.min(m.length - 1, lastRow * cols - 1))
    }
    function listWindowReport() {
        var first = Math.max(0, Math.floor(list.contentY / 44) - 4)
        var last = Math.ceil((list.contentY + list.height) / 44) + 4
        queueWindowReport(first, Math.min(root.visibleRows.length - 1, last))
    }

    // ============================ overlay =================================
    // Declared LAST: declaration order IS z-order, so the popup sits above
    // the toolbar and the grid and keeps its own presses. Hidden AND
    // disabled while closed, so it never eats a click meant for the view.
    // Its "library-all" context is independent from Discover's "discover"
    // one — each button carries its own count.
    GenreFilterPopup {
        id: libGenrePopup
        anchors.fill: parent
        context: "library-all"
        // Under the 56px toolbar, right-aligned like the Slint overlay.
        anchorTop: 62
        anchorRight: 32
    }
}
