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
// THIS FILE IS THE STATE + ORCHESTRATION HALF. The bodies live next door
// (track rule 2 — it used to be 1,881 lines):
//   library/LibraryToolbar.qml      the whole 56px chrome band
//   library/FeedListRow.qml         the All feed's mixed LIST row
//   library/LibraryAlbumsList.qml   Albums in LIST mode + its bulk bar
//   library/LibraryArtistsPanel.qml Artists in SIDEPANEL mode
// Each takes `view: root`, the seam views/local/*.qml already uses.
//
// Still out of scope, and named rather than implied:
// - Offline: the generic OfflinePlaceholder replica mounts (the Slint
//   offline RAIL of playable cached favorites needs the offline cache —
//   not wired; PARITY-DEBT #17).
// - The Library "All" local SCOPE ("favorites" vs the whole local library)
//   is PARITY-DEBT #6: the pref is read and PRESERVED on disk by
//   library_prefs.rs, but neither the Settings row nor the `all` branch of
//   the feed exists here yet.
//
// Filter by genre IS wired, on the All / Tracks / Albums toolbars — the three
// places the Slint FavoritesView draws it (FavoritesView.slint:609, :652,
// :864, `context: "library-all"`). It filters CLIENT-side over the feed,
// exactly like library_all.rs::derive: an item shows when its (lowercased)
// genre contains one of the selected genre names (+ their sub-genres), so rows
// with no genre at all (artist / label / playlist) drop out while a genre is
// selected. The selection lives in the shared per-context store and persists
// to genre_filter.json.
//
// Toolbar choices PERSIST (favorites_prefs.rs parity): albums view/sort/group,
// tracks group, playlists view, artists group/view and the All hard-drive
// toggle all go through `setPref` into the same favorites_ui.json the shipping
// Slint build writes. Search queries stay transient — deliberately, there too.

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "../controls"
import "../rows"
import "../theme"
import "library"

Rectangle {
    id: root
    // Transparent while the ambient background is active (phase 14 —
    // HomeView.slint:163: the frosted content panel shows through).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: theme.ambientOn

    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

    QbzTheme { id: theme }

    // ============================ state ==================================
    // Everything that logs through this category MUST defer the call (see the
    // `Qt.callLater` in `visibleRows` and `parseFeed`). Declaring it first is
    // NOT enough and moving it here did not silence the warning: a category is
    // finalised the first time it is USED, so a `console.info` running during
    // component creation pins it before Qt applies `defaultLogLevel`, which
    // then warns ("cannot be changed after the component is completed") and —
    // the part that matters — leaves the level UNSET, i.e. this logging on by
    // default for every user. Deferring past creation is what actually fixes
    // it. Both call sites below run during creation, which is why both defer.
    LoggingCategory {
        id: libTiming
        name: "qbz.nav.timing"
        defaultLogLevel: LoggingCategory.Warning
    }

    property string activeTab: "all"

    readonly property var counts: JSON.parse(QbzLibrary.libraryCountsJson)
    // Full merged feed (parsed once per publish — timed for the report).
    readonly property var feed: parseFeed(QbzLibrary.libraryJson)
    function parseFeed(json) {
        var t = Date.now()
        var f = JSON.parse(json)
        // Was a bare console.log, i.e. printed on every publish for every user
        // forever. It belongs to the same investigation as the derive timing
        // right below it, so it now rides the same category and is silent
        // unless that category is turned on.
        var line = "[libtiming] JSON.parse feed: " + (Date.now() - t)
                 + "ms (" + f.length + " items, " + json.length + " bytes)"
        Qt.callLater(function () { console.info(libTiming, line) })
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
    // Group modes (favorites.rs derive_tracks / derive_albums / artists).
    // Grouping REORDERS rows; for the tracks list it also injects separator
    // rows — see visibleItems / withGroupHeaders.
    property string tracksGroup: "off"   // "off" | "album" | "artist" | "name"
    property string albumsGroup: "off"   // "off" | "alpha" | "artist"
    property string artistsGroup: "off"  // "off" | "alpha"
    property string sortBy: "date"      // "date" | "title" | "artist"
    property bool sortAsc: false        // date: false = newest first
    property bool showPurchases: true
    property bool showFavorites: true
    property bool showFollowing: true
    property bool showLocal: true
    property string viewMode: "grid"    // All tab: "grid" | "list"
    property string albumsView: "grid"    // "grid" | "list"
    property string playlistsView: "grid" // "grid" | "list"
    property string artistsView: "grid"   // "grid" | "sidepanel"

    // --- persisted toolbar choices (library_prefs.rs) --------------------
    // Seeded from the bridge document rather than BOUND to it: these are
    // user-editable properties, and a binding would fight every click. The
    // Rust setter never republishes the document, so there is no loop.
    function applyPrefs() {
        var p
        try {
            p = JSON.parse(QbzLibrary.libraryPrefsJson)
        } catch (e) {
            return
        }
        // The default "{}" reaches here before `boot` seeds the real one.
        if (!p || p.albumsView === undefined) return
        root.albumsView = p.albumsView
        root.albumsSort = p.albumsSort
        root.albumsGroup = p.albumsGroup
        root.tracksGroup = p.tracksGroup
        root.playlistsView = p.playlistsView
        root.artistsGroup = p.artistsGroup
        root.artistsView = p.artistsView
        root.showLocal = p.allShowLocal === true
    }
    Component.onCompleted: root.applyPrefs()
    Connections {
        target: QbzLibrary
        function onLibraryPrefsJsonChanged() { root.applyPrefs() }
    }

    /// Write one toolbar choice AND persist it. The toolbar calls this instead
    /// of assigning the property, so the two can never drift apart.
    function setPref(key, value) {
        if (key === "albumsView") {
            root.albumsView = value
            // Multi-select is LIST-only (FavoritesView.slint:514); leaving
            // list mode with a live selection would strand the bulk bar.
            if (value !== "list") root.setAlbumsMultiSelect(false)
        } else if (key === "albumsSort") root.albumsSort = value
        else if (key === "albumsGroup") root.albumsGroup = value
        else if (key === "tracksGroup") root.tracksGroup = value
        else if (key === "playlistsView") root.playlistsView = value
        else if (key === "artistsGroup") root.artistsGroup = value
        else if (key === "artistsView") root.artistsView = value
        else {
            console.warn("[qbz-qt] LibraryView.setPref: unknown key " + key)
            return
        }
        QbzLibrary.setLibraryPref(key, String(value))
    }
    /// The one SOURCE switch the reference persists (favorites_prefs.rs
    /// `all_show_local`); the other three are session-local there too.
    function setShowLocal(on) {
        root.showLocal = on
        QbzLibrary.setLibraryPref("allShowLocal", on ? "true" : "false")
    }

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
    /// The toolbar's genre buttons reach the popup through here — it is
    /// declared LAST in this file and cannot be named from a child component.
    function toggleGenrePopup() { libGenrePopup.toggle() }

    /// The All tab's filters as the applied-filters tooltip wants them: the
    /// selected genres, plus the four SOURCE switches — but only when one is
    /// OFF, because all four on is the default and "everything is shown" is not
    /// a filter worth naming. Genres come back in display casing here (the
    /// lowercased copy above exists for matching, not for reading).
    readonly property var filterSummaryGroups: {
        var tr = QbzSession.trRev
        var hidden = []
        if (!root.showFavorites)
            hidden.push(QbzSession.tr("Favorites", tr))
        if (!root.showPurchases)
            hidden.push(QbzSession.tr("Purchases", tr))
        if (!root.showFollowing)
            hidden.push(QbzSession.tr("Following", tr))
        if (!root.showLocal)
            hidden.push(QbzSession.tr("Local", tr))
        var out = []
        if (hidden.length > 0)
            out.push({ group: QbzSession.tr("Hidden", tr), values: hidden })
        return out
    }

    // Other-tab state.
    property string tabSearch: ""
    property string albumsSort: "default" // default|title-asc|title-desc|artist-asc
    property string playlistsSubTab: "favorites"

    /// The All tab's search field debounces; the toolbar routes through here
    /// so the timer stays next to the window report it feeds.
    function setAllSearch(v) {
        root.search = v
        allSearchDebounce.restart()
    }

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
        if (toolbar) toolbar.closeTabSearch()
        // A selection cannot survive leaving its own tab: the bulk bar would
        // still be armed over rows that are no longer on screen.
        setTracksMultiSelect(false)
        setAlbumsMultiSelect(false)
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
    // Timed like `parseFeed` above, and for the same reason: this runs a full
    // pass plus a sort over the WHOLE merged feed, so it is the one derive in
    // the app whose cost scales with the size of the user's library. The log
    // rides the shared route-timing category — it is silent unless
    // QT_LOGGING_RULES="qbz.nav.timing.info=true".
    readonly property var visibleRows: {
        var _t = Date.now()
        var _rows = visibleItems()
        var _line = "[libtiming] derive tab=" + activeTab + " feed=" + feed.length
                  + " rows=" + _rows.length + " in " + (Date.now() - _t) + "ms"
        Qt.callLater(function () { console.info(libTiming, _line) })
        return _rows
    }

    /// A-Z bucket for a title: its first letter uppercased, "#" for anything
    /// that is not a letter (a leading digit, "*", punctuation). Matches the
    /// jump strip's own buckets, so a letter in the strip always has a
    /// separator to land on.
    function alphaKey(title) {
        var t = (title || "").trim()
        if (t.length === 0) return "#"
        var c = t.charAt(0).toUpperCase()
        return (c >= "A" && c <= "Z") ? c : "#"
    }

    /// Inject a `group-header` pseudo-row before each run of rows sharing a
    /// group key. The rows are ALREADY ordered by that key (see the `by(...)`
    /// sorts above), so this is one linear pass.
    ///
    /// Album and artist grouping match the reference's separators. NAME
    /// grouping gets them too — the reference draws only the A-Z jump strip
    /// there and no separators, which the owner asked to close (2026-07-31):
    /// a strip that jumps to a letter with nothing labelling it leaves the
    /// user guessing where a bucket starts.
    function withGroupHeaders(rows, mode) {
        var out = []
        var last = null
        var n = 1
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i]
            var key = mode === "album" ? (r.album || "")
                : mode === "artist" ? (r.artist || "")
                : alphaKey(r.title)
            if (key !== last) {
                out.push({ "kind": "group-header", "title": key, "id": "hdr:" + mode + ":" + key })
                last = key
            }
            // The visible track NUMBER keeps running across groups (the
            // reference numbers 1,2,3… straight through its separators), so it
            // cannot be the model index once headers share the model. Carried
            // on a shallow COPY — the feed's row objects are shared with the
            // grid and the artwork map and must not be mutated.
            out.push(Object.assign({}, r, { "_no": n }))
            n++
        }
        return out
    }

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
        var tabGenres = (activeTab === "tracks" || activeTab === "albums")
            ? root.genreNames : []
        for (i = 0; i < feed.length; i++) {
            var x = feed[i]
            var keep = false
            if (activeTab === "tracks") keep = x.kind === "track" && x.group === "favorites"
            else if (activeTab === "albums") keep = x.kind === "album" && x.group === "favorites"
            else if (activeTab === "artists") keep = x.kind === "artist"
            else if (activeTab === "labels") keep = x.kind === "label"
            else if (activeTab === "playlists") keep = x.kind === "playlist"
                && (playlistsSubTab === "following" ? x.group === "following" : x.group === "favorites")
            if (keep && tabGenres.length > 0) {
                var tg = (x.genre || "").toLowerCase()
                keep = false
                for (var tgi = 0; tgi < tabGenres.length; tgi++) {
                    if (tg !== "" && tg.indexOf(tabGenres[tgi]) >= 0) { keep = true; break }
                }
            }
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
        // --- Group-by (favorites.rs::derive_tracks / derive_albums) ------
        // Grouping REORDERS the rows so a group's entries sit together. The
        // TRACKS list also gets separator rows (see withGroupHeaders); the
        // albums and artists GRIDS do not — a uniform-cell GridView has no
        // room for a full-width header, which is exactly why the reference
        // scrolls those two PROPORTIONALLY from the A-Z strip instead of
        // landing on a separator (FavoritesView.slint:1996, :2008).
        var lc = function (s) { return (s || "").toLowerCase() }
        var by = function (keys) {
            items.sort(function (a, b) {
                for (var k = 0; k < keys.length; k++) {
                    var av = lc(a[keys[k]]), bv = lc(b[keys[k]])
                    if (av !== bv) return av < bv ? -1 : 1
                }
                return 0
            })
        }
        if (activeTab === "tracks" && tracksGroup !== "off") {
            if (tracksGroup === "album") by(["album", "title"])
            else if (tracksGroup === "artist") by(["artist", "album", "title"])
            else if (tracksGroup === "name") by(["title"])
            items = withGroupHeaders(items, tracksGroup)
        } else if (activeTab === "albums" && albumsGroup !== "off") {
            // "alpha" is the album title's initial, which title-order gives
            // for free; "artist" groups by artist then title.
            if (albumsGroup === "alpha") by(["title"])
            else if (albumsGroup === "artist") by(["artist", "title"])
        } else if (activeTab === "artists" && artistsGroup === "alpha") {
            by(["title"])
        }
        return items
    }

    // ------------------------ A-Z jump strip -----------------------------
    // FavoritesView.slint:1680-1691 (tracks, name grouping — index-accurate
    // because the rows are uniform), :1988-1999 (albums, alpha grouping) and
    // :2000-2011 (artists, alpha grouping).
    //
    // ONE deliberate improvement over the reference on the two GRIDS: it
    // scrolls proportionally (`ord / (jumps-1) * maxScroll`) because a Slint
    // Flickable cannot address a cell; a QML GridView can, so the strip lands
    // on the letter's FIRST card instead of near it. Same strip, same
    // buckets, an exact landing.
    readonly property bool alphaActive:
        (activeTab === "tracks" && tracksGroup === "name")
        || (activeTab === "albums" && albumsGroup === "alpha")
        || (activeTab === "artists" && artistsGroup === "alpha" && artistsView === "grid")
    readonly property var alphaJumps: {
        if (!root.alphaActive) return []
        var rows = root.visibleRows
        var out = []
        var last = null
        for (var i = 0; i < rows.length; i++) {
            // The tracks list already carries a separator per bucket; jumping
            // to the SEPARATOR (not the first track) is what puts the letter
            // itself at the top of the viewport.
            if (activeTab === "tracks") {
                if (rows[i].kind !== "group-header") continue
                out.push({ "letter": rows[i].title, "index": i })
                continue
            }
            var key = root.alphaKey(rows[i].title)
            if (key !== last) {
                out.push({ "letter": key, "index": i })
                last = key
            }
        }
        return out
    }
    readonly property bool alphaVisible: root.alphaActive && root.alphaJumps.length > 0
    function alphaJumpTo(index) {
        if (activeTab === "tracks") list.positionViewAtIndex(index, ListView.Beginning)
        else if (activeTab === "albums" && albumsView === "list") albumsList.positionAt(index)
        else grid.positionViewAtIndex(index, GridView.Beginning)
    }

    // ------------------------ multi-select -------------------------------
    // FavoritesView.slint:465-523 (the two toggles) + :1570 / :1734 (the two
    // bars). The SELECTION lives here, in QML, exactly like the Local Library
    // grid/table bars: "select-all" and "clear" never reach Rust, everything
    // else goes down as a JSON id array (src/library_bulk.rs).
    property bool tracksMultiSelect: false
    property var tracksSelected: ({})
    readonly property int tracksSelectedCount: Object.keys(root.tracksSelected).length
    property bool albumsMultiSelect: false
    property var albumsSelected: ({})
    readonly property int albumsSelectedCount: Object.keys(root.albumsSelected).length

    function setTracksMultiSelect(on) {
        root.tracksMultiSelect = on
        if (!on) root.tracksSelected = ({})
    }
    function setAlbumsMultiSelect(on) {
        root.albumsMultiSelect = on
        if (!on) root.albumsSelected = ({})
    }
    // A rebind needs a NEW object reference — mutating the map in place
    // notifies nothing (the same rule artMap and every row's `favorite`
    // property follow).
    function toggleTrackSelected(id) {
        var m = root.tracksSelected
        if (m[id] === true) delete m[id]
        else m[id] = true
        root.tracksSelected = Object.assign({}, m)
    }
    function toggleAlbumSelected(id) {
        var m = root.albumsSelected
        if (m[id] === true) delete m[id]
        else m[id] = true
        root.albumsSelected = Object.assign({}, m)
    }

    /// Selected ids in VISIBLE order — never `Object.keys(map)`. Qobuz ids are
    /// numeric strings, and a JS object iterates integer-like keys in ASCENDING
    /// NUMERIC order, so the keys of the map are not the order the user sees
    /// and "play next" would insert a re-sorted block.
    function selectedIdsInOrder(kind, map) {
        var rows = root.visibleRows
        var out = []
        for (var i = 0; i < rows.length; i++)
            if (rows[i].kind === kind && map[rows[i].id] === true) out.push(rows[i].id)
        return out
    }

    function tracksBulkAction(action) {
        if (action === "select-all") {
            var m = {}
            var rows = root.visibleRows
            for (var i = 0; i < rows.length; i++)
                if (rows[i].kind === "track") m[rows[i].id] = true
            root.tracksSelected = m
            return
        }
        if (action === "clear") { root.tracksSelected = ({}); return }
        var ids = root.selectedIdsInOrder("track", root.tracksSelected)
        if (ids.length === 0) return
        QbzLibrary.libraryBulkAction("track", JSON.stringify(ids), action)
        // The Slint clears after an enqueue and after remove-selected, and
        // KEEPS the selection while a picker is still open (a failed write is
        // retried from the same modal) — local_bulk.rs::apply says the same.
        if (action !== "add-to-playlist" && action !== "add-to-mixtape")
            root.tracksSelected = ({})
    }
    function albumsBulkAction(action) {
        if (action === "select-all") {
            var m = {}
            var rows = root.visibleRows
            for (var i = 0; i < rows.length; i++)
                if (rows[i].kind === "album") m[rows[i].id] = true
            root.albumsSelected = m
            return
        }
        if (action === "clear") { root.albumsSelected = ({}); return }
        var ids = root.selectedIdsInOrder("album", root.albumsSelected)
        if (ids.length === 0) return
        QbzLibrary.libraryBulkAction("album", JSON.stringify(ids), action)
        root.albumsSelected = ({})
    }

    // --- Ctrl+A / Escape hotkeys interface (2026-08-03 hotkeys-port §4.6) --
    // The duck-typed seam the AppShell router calls on QbzShell's
    // selectAllRequested / exitMultiSelectRequested. Only the tracks and
    // albums tabs have multi-select here; on any other tab selectAll() is a
    // deliberate no-op (the capability reporter still counts this view, so
    // Ctrl+A consumes without visible effect on the All/Artists/Playlists
    // tabs — those have no selection model to fill).
    readonly property bool multiSelectOn: root.tracksMultiSelect || root.albumsMultiSelect
    function selectAll() {
        if (root.activeTab === "tracks") {
            if (!root.tracksMultiSelect) root.setTracksMultiSelect(true)
            root.tracksBulkAction("select-all")
        } else if (root.activeTab === "albums") {
            if (!root.albumsMultiSelect) root.setAlbumsMultiSelect(true)
            root.albumsBulkAction("select-all")
        }
    }
    function exitMultiSelectMode() {
        if (root.tracksMultiSelect) root.setTracksMultiSelect(false)
        if (root.albumsMultiSelect) root.setAlbumsMultiSelect(false)
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
        // A group-header pseudo-row has no artwork of its own; skipping it here
        // keeps `undefined` out of the keep-set and the pending count.
        for (var i = keepLo; i <= keepHi; i++)
            if (visibleArray[i].kind !== "group-header") keep[visibleArray[i].artKey] = true
        var m = artMap
        var pending = 0
        for (i = first; i <= last; i++) {
            if (visibleArray[i].kind === "group-header") continue
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
        // ArtistCard.qml) and the list rows do the same
        // (library/FeedListRow.qml).
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
    //
    // `pendingRows` is the array the band indexes into. It is null for every
    // body that scrolls `visibleRows` itself (grid, list, albums list,
    // playlists list) and non-null ONLY for a body whose mounted order is not
    // `visibleRows` — today that is the Artists sidepanel rail, which sorts
    // A-Z and interleaves letter headers, so its ListView index N is NOT
    // `visibleRows[N]`. Reporting the rail against `visibleRows` would name
    // the wrong artists; reporting nothing (what it did until now) left
    // `artMap` empty for every row it mounts and every rail avatar fell to
    // the placeholder.
    Timer {
        id: windowDebounce
        interval: 180
        onTriggered: root.reportWindow(
            pendingRows ? pendingRows : root.visibleRows, pendingFirst, pendingLast)
        property int pendingFirst: 0
        property int pendingLast: 0
        property var pendingRows: null
    }
    /// `rows` is optional — omit it and the band is read off `visibleRows`.
    function queueWindowReport(first, last, rows) {
        windowDebounce.pendingFirst = first
        windowDebounce.pendingLast = last
        windowDebounce.pendingRows = rows === undefined ? null : rows
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
        QbzLibrary.libraryPlayVisible(JSON.stringify(visibleTrackIds()), trackId)
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

        LibraryToolbar {
            id: toolbar
            width: parent.width
            view: root
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ===================== SCROLLING CONTENT =========================
        Item {
            id: content
            width: parent.width
            height: parent.height - 57
            clip: true

            // The tracks bulk bar is pinned above the list (the Slint scrolls
            // it away with the page; here the list IS the scrolling view).
            readonly property int barInset:
                (root.activeTab === "tracks" && root.tracksMultiSelect) ? 52 : 0

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
                    || (root.activeTab === "albums" && root.albumsView === "list")
                    || (root.activeTab === "playlists" && root.playlistsView === "list")
                readonly property int cols: Math.max(1, Math.floor(width / 220))
                readonly property int rows: Math.max(1, Math.ceil(height / 266))
                readonly property int rowHeight: root.activeTab === "tracks" ? 50
                    : root.activeTab === "albums" ? 64
                    : root.activeTab === "playlists" ? 60 : 44

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

            readonly property bool ready:
                !QbzLibrary.libraryLoading && QbzLibrary.libraryError === ""
            // Which surface owns the tab right now — one predicate per body,
            // so no two can mount at once.
            readonly property bool showAlbumsList:
                content.ready && root.activeTab === "albums" && root.albumsView === "list"
            readonly property bool showPlaylistsList:
                content.ready && root.activeTab === "playlists" && root.playlistsView === "list"
            readonly property bool showArtistsPanel:
                content.ready && root.activeTab === "artists" && root.artistsView === "sidepanel"
            readonly property bool showList:
                content.ready && ((root.activeTab === "all" && root.viewMode === "list")
                                  || root.activeTab === "tracks")
            readonly property bool showGrid:
                content.ready && !content.showList && !content.showAlbumsList
                && !content.showPlaylistsList && !content.showArtistsPanel

            // ============ GRID (all tabs; All in grid mode) ==============
            // HEIGHT-GATED, and that is the load-bearing part — see the
            // `showGrid`/`showList`/... predicates above, which already say
            // "one predicate per body, so no two can mount at once". They did
            // not achieve that: `visible: false` stops a view from PAINTING,
            // but a QQuickItemView that still has a viewport and a model keeps
            // refilling it. Measured 2026-08-17 with a per-view creation
            // counter: on the Albums tab the grid built 51 delegates and the
            // hidden list built 47 alongside it; on Tracks the list built 47
            // and the hidden GRID built 51 — the expensive direction, since a
            // FeedGridCell carries a cover. Half of every Library mount was
            // being spent on the body that was not showing.
            //
            // Collapsing the height to 0 is what actually stops the refill.
            // The obvious alternative — `model: visible ? root.visibleRows :
            // []` — is the one thing this file spends a paragraph warning
            // against (see onLibraryFavoriteChanged): handing `model` a new
            // value goes through QQuickItemView::setModel(), which resets the
            // scroll offset to 0 and rebuilds every delegate. The height gate
            // leaves `model` untouched.
            GridView {
                id: grid
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 32
                anchors.rightMargin: root.alphaVisible ? 52 : 32
                anchors.topMargin: 16
                height: grid.visible ? parent.height - 16 : 0
                visible: content.showGrid && root.activeTab !== "tracks"
                cellWidth: 220
                cellHeight: 266
                cacheBuffer: 266 * 2
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleRows

                onContentYChanged: root.gridWindowReport()
                onModelChanged: root.gridWindowReport()
                onWidthChanged: root.gridWindowReport()
                onVisibleChanged: root.gridWindowReport()
                Component.onCompleted: root.gridWindowReport()

                delegate: FeedGridCell {
                    required property var modelData
                    required property int index
                    view: root
                    item: modelData
                    // Viewport-relative so the skeleton's 48-instance cap
                    // follows the window, not the (possibly 10k-item) model.
                    cellIndex: Math.max(0, index - root.artWindowFirst)
                }
            }

            // ============ LIST (All list mode + Tracks tab) ==============
            // Height-gated for the reason spelled out on the grid above.
            ListView {
                id: list
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 32
                anchors.rightMargin: root.alphaVisible ? 52 : 32
                anchors.topMargin: 10 + content.barInset
                height: list.visible ? parent.height - (10 + content.barInset) : 0
                visible: content.showList
                cacheBuffer: 44 * 10
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleRows
                onContentYChanged: root.listWindowReport()
                onModelChanged: root.listWindowReport()
                onVisibleChanged: root.listWindowReport()
                Component.onCompleted: root.listWindowReport()

                delegate: Loader {
                    required property var modelData
                    required property int index
                    width: list.width
                    height: modelData && modelData.kind === "group-header"
                        ? 34
                        : (root.activeTab === "tracks" ? 50 : 44)
                    Component {
                        id: listHeaderComp
                        Item {
                            Text {
                                x: 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 4
                                text: modelData.title
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightSemibold
                                elide: Text.ElideRight
                            }
                        }
                    }
                    Component {
                        id: trackRowComp
                        TrackRow {
                            item: modelData
                            number: modelData._no || (index + 1)
                            selectMode: root.tracksMultiSelect
                            checked: root.tracksSelected[modelData.id] === true
                            onToggleSelect: root.toggleTrackSelected(modelData.id)
                            onPlayRequested: root.playTrackInContext(item.id)
                            onEnqueueRequested: function (m) { QbzPlayer.enqueueTrack(item.id, m) }
                            // MyQBZ "Add to mixtape" — the HOST builds the
                            // AddItem array (TrackRow does not know
                            // itemType/source).
                            //
                            // The source comes off the ROW: every feed row
                            // carries its own word (library_qt.rs:54 —
                            // "qobuz" | "local" | "plex"), so a Plex or local
                            // row can never be stored as a Qobuz id again.
                            // "plex" is folded into "local" because the store
                            // has no Plex source: `AddItem.source` is
                            // "qobuz" | "local" and `source_from_str`
                            // (myqbz_add_qt.rs:85-90) maps everything that is
                            // not "local" to Qobuz — passing "plex" verbatim
                            // would silently become "qobuz" again.
                            //
                            // Today this list only ever holds Qobuz rows (the
                            // Tracks tab keeps `kind === "track" && group ===
                            // "favorites"` and the local layer lands in group
                            // "local"), so this is a no-op until that filter
                            // changes — which is the point: it stays right
                            // when it does.
                            onMixtapeRequested: QbzMyQbzAdd.open(JSON.stringify([{
                                "itemType": "track",
                                "source": item.source === "local" || item.source === "plex"
                                    ? "local" : "qobuz",
                                "sourceItemId": item.id, "title": item.title || "",
                                "subtitle": item.artist || "", "artworkUrl": "",
                                "year": null, "trackCount": null
                            }]))
                        }
                    }
                    Component {
                        id: feedRowComp
                        FeedListRow {
                            view: root
                            item: modelData
                            rowIndex: Math.max(0, index - root.artWindowFirst)
                        }
                    }
                    sourceComponent: (modelData && modelData.kind === "group-header")
                        ? listHeaderComp
                        : (root.activeTab === "tracks" ? trackRowComp : feedRowComp)
                }
            }

            // ============ Albums LIST mode ===============================
            LibraryAlbumsList {
                id: albumsList
                visible: content.showAlbumsList
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: root.alphaVisible ? 52 : 32
                anchors.topMargin: 16
                view: root
                rows: content.showAlbumsList ? root.visibleRows : []
            }

            // ============ Playlists LIST mode ============================
            // FavoritesView.slint:1935 — the same ViewToggle the albums tab
            // has, over primitives/PlaylistListRow.
            ListView {
                id: playlistsList
                visible: content.showPlaylistsList
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 16
                clip: true
                spacing: 2
                cacheBuffer: 60 * 8
                boundsBehavior: Flickable.StopAtBounds
                model: content.showPlaylistsList ? root.visibleRows : []

                // Gated on visibility for the same reason gridWindowReport and
                // listWindowReport are: `artMap` is ONE map, a report PRUNES
                // outside its band, and this body still fires while hidden —
                // `model` flips to [] on every tab switch (onModelChanged) and
                // `anchors.fill` makes every window resize an onHeightChanged.
                // Ungated, a resize on the Artists sidepanel evicted the rail's
                // avatars and requested a `visibleRows` band the rail is not
                // even showing (its order is A-Z, not feed order).
                function report() {
                    if (!playlistsList.visible) return
                    var first = Math.max(0, Math.floor(playlistsList.contentY / 62) - 4)
                    var last = Math.ceil((playlistsList.contentY + playlistsList.height) / 62) + 4
                    root.queueWindowReport(first, Math.min(root.visibleRows.length - 1, last))
                }
                onContentYChanged: playlistsList.report()
                onModelChanged: playlistsList.report()
                onHeightChanged: playlistsList.report()
                onVisibleChanged: playlistsList.report()
                Component.onCompleted: playlistsList.report()

                delegate: PlaylistListRow {
                    required property var modelData
                    required property int index
                    width: playlistsList.width
                    item: modelData
                    rowIndex: index
                    artSource: root.artMap[modelData.artKey] || ""
                    covers: modelData.playlistOwnImage === true ? [] : (modelData.covers || [])
                }
            }

            // ============ Artists SIDEPANEL mode =========================
            // Behind a Loader so LEAVING the mode really destroys it: the
            // panel drops its Rust-side selection in Component.onDestruction,
            // and it also stops rebuilding its rail on every feed change while
            // the user is on another tab.
            Loader {
                anchors.fill: parent
                active: content.showArtistsPanel
                sourceComponent: LibraryArtistsPanel { view: root }
            }

            // Declared AFTER every scrolling body: declaration order IS
            // z-order, and this bar overlays the top of the content area.
            // ---- Tracks multi-select bar (FavoritesView.slint:1570) -----
            QbzMultiSelectBar {
                id: tracksBar
                visible: root.activeTab === "tracks" && root.tracksMultiSelect && content.ready
                anchors.top: parent.top
                anchors.topMargin: 10
                anchors.left: parent.left
                anchors.leftMargin: 32
                anchors.right: parent.right
                anchors.rightMargin: 32
                selectedCount: root.tracksSelectedCount
                // The reference's inventory MINUS "Make available offline":
                // this port brings up no offline cache at all (src/
                // library_bulk.rs names the check), and a bulk bar must not
                // render a control that no-ops. Everything else is wired.
                // "heart-crack" is likewise not in this port's icon set, so
                // "Remove from Library" uses the same heart + danger pair
                // views/local/LocalTracksTab.qml already ships.
                actions: [
                    { "id": "select-all", "label": QbzSession.tr("Select all", QbzSession.trRev), "icon": "square-check-big", "danger": false, "needsSelection": false },
                    { "id": "play-next", "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "danger": false, "needsSelection": true },
                    { "id": "play-later", "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "danger": false, "needsSelection": true },
                    { "id": "queue", "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "danger": false, "needsSelection": true },
                    { "id": "add-to-playlist", "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "danger": false, "needsSelection": true },
                    { "id": "add-to-mixtape", "label": QbzSession.tr("Add to Mixtape/Collection", QbzSession.trRev), "icon": "cassette-tape", "danger": false, "needsSelection": true },
                    { "id": "remove-selected", "label": QbzSession.tr("Remove from Library", QbzSession.trRev), "icon": "heart", "danger": true, "needsSelection": true },
                    { "id": "clear", "label": QbzSession.tr("Clear", QbzSession.trRev), "icon": "x", "danger": false, "needsSelection": true },
                ]
                onAction: function (id) { root.tracksBulkAction(id) }
            }

            // A-Z jump strip, right edge (FavoritesView.slint:1683 / :1992 /
            // :2004 all pin it at `parent.width - self.width - 20px`).
            QbzAlphaStrip {
                visible: root.alphaVisible
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.top: parent.top
                anchors.topMargin: 16
                anchors.bottom: parent.bottom
                jumps: root.alphaJumps
                onJump: function (ordinal, index) { root.alphaJumpTo(index) }
            }

            // Thin auto-hiding scrollbars (ListScrollbar replica).
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: root.alphaVisible ? 34 : 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: grid
                visible: grid.visible && grid.contentHeight > grid.height
            }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: root.alphaVisible ? 34 : 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: list
                visible: list.visible && list.contentHeight > list.height
            }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: playlistsList
                visible: playlistsList.visible
                    && playlistsList.contentHeight > playlistsList.height
            }
        }
    }

    // Both run from onContentYChanged, i.e. from INSIDE QQuickItemView::
    // setModel() — see the visibleRows comment. Everything they touch on the
    // view (width/height/contentY/cellWidth/cellHeight) is a plain qreal on
    // the private; the array comes from root, never from the view.
    //
    // Both early-return while their body is hidden. `artMap` is ONE map and a
    // report PRUNES everything outside the reported band, so a hidden body
    // that still fires (the grid's rightMargin changes when `alphaVisible`
    // flips; the list's `onModelChanged` fires on every tab switch because its
    // `model` is bound to `visibleRows` unconditionally) would evict the
    // covers of whichever body is actually on screen — the Artists sidepanel
    // rail above all, since it is the one body whose band is NOT a
    // `visibleRows` band. `onVisibleChanged` keeps the gate from swallowing
    // the first report of a body that becomes visible after its model landed.
    function gridWindowReport() {
        if (!grid.visible) return
        var cols = Math.max(1, Math.floor(grid.width / grid.cellWidth))
        var firstRow = Math.max(0, Math.floor(grid.contentY / grid.cellHeight) - 1)
        var lastRow = Math.ceil((grid.contentY + grid.height) / grid.cellHeight) + 1
        var m = root.visibleRows
        queueWindowReport(firstRow * cols, Math.min(m.length - 1, lastRow * cols - 1))
    }
    function listWindowReport() {
        if (!list.visible) return
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
