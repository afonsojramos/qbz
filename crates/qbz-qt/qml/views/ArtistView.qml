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
//
// Header atmosphere (ArtistPageView.slint:120-147, 211-247): wired through
// the shared controls/HeaderGradient.qml — the SAME component AlbumView
// mounts, because the .slint paints both headers from identical blocks. It
// carries the .slint's header-colour rules with it (light text + overlay
// CircleActions while the band is on).

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

    readonly property var artist: JSON.parse(QbzArtist.artistJson)

    // ---- In-page search (JumpNavBar's magnifier) -------------------------
    // artist.rs `filter_artist` (main.rs:14991 wires ArtistActions.search to
    // it) is a PURE client-side filter over Popular Tracks, Appears On and
    // every release-section album — no backend round-trip — so it ports as a
    // filter over the parsed document rather than a bridge call.
    property string searchQuery: ""
    readonly property string needle: searchQuery.trim().toLowerCase()

    function matchTrack(t) {
        return needle === ""
            || (t.title || "").toLowerCase().indexOf(needle) >= 0
            || (t.artist || "").toLowerCase().indexOf(needle) >= 0
    }

    readonly property var topTracks: {
        var all = artist.topTracks || []
        if (root.needle === "") return all
        var out = []
        for (var i = 0; i < all.length; i++) if (matchTrack(all[i])) out.push(all[i])
        return out
    }
    readonly property var appearsOn: {
        var all = artist.appearsOn || []
        if (root.needle === "") return all
        var out = []
        for (var i = 0; i < all.length; i++) if (matchTrack(all[i])) out.push(all[i])
        return out
    }
    // A section whose albums all filter out DISAPPEARS (artist.rs:1033), and
    // Load more is suppressed while a filter is active (:1042 — appending
    // would bring back unfiltered items).
    readonly property var releaseSections: {
        var all = artist.releaseSections || []
        if (root.needle === "") return all
        var out = []
        for (var i = 0; i < all.length; i++) {
            var kept = []
            var cards = all[i].cards || []
            for (var j = 0; j < cards.length; j++)
                if ((cards[j].title || "").toLowerCase().indexOf(root.needle) >= 0)
                    kept.push(cards[j])
            if (kept.length === 0) continue
            out.push({ "releaseType": all[i].releaseType, "title": all[i].title,
                       "cards": kept, "hasMore": false })
        }
        return out
    }
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

    // ---- Header atmosphere (ArtistPageView.slint:120-147) ----------------
    // Same three-line rule as AlbumView (the .slint says "same rule as
    // AlbumPageView" at :120). The pref is read LIVE off the settings
    // snapshot where one exists, else off the document (artist_qt.rs).
    readonly property bool headerGradientPref: {
        var raw = QbzBridge.settingsJson
        if (raw && raw.length > 2) {
            try {
                var d = JSON.parse(raw)
                if (d.albumHeaderGradient !== undefined)
                    return d.albumHeaderGradient === true
            } catch (e) { /* fall through to the document copy */ }
        }
        return artist.headerGradient !== false
    }
    readonly property bool headerAtmoOn: headerGradientPref && !ambientOn
    readonly property bool headerLight: headerGradientPref || ambientOn
    readonly property color hdrStrong: headerLight ? "#ffffff" : theme.textPrimary
    readonly property color hdrBody: headerLight ? "#e0ffffff" : theme.textSecondary
    readonly property bool hdrOverlay: headerLight
    /// Slint's `Theme.text-primary` as an ICON tint, for the hovers that raise
    /// a muted glyph on a THEME surface (every consumer sits on
    /// `surface-elevated`/transparent, never on the artwork header — that one
    /// uses `hdrStrong` above). Runtime-tinted via src/icon_tint_qt.rs, so it
    /// is the live token; it used to be `isDark ? "primary" : "black"`, a
    /// two-value stand-in from when only fixed bakes existed.
    readonly property string tintOnSurface: "textPrimary"
    /// TrackRow.slint:123-125 — the row hover uses the polarity-baked alpha
    /// ramp "so the hover state is visible on light themes too (the old
    /// #ffffff16 was invisible white-on-white there)". The zebra stripe is
    /// deliberately left as the literal, per the same comment.
    readonly property color rowHoverBg: theme.alphaTiers.length > 0
        ? theme.alphaTier(8) : (theme.isDark ? "#14ffffff" : "#14000000")
    property bool topTracksExpanded: false
    property bool appearsOnExpanded: false
    property bool otherExpanded: false

    // ---- Network sidebar open/closed -------------------------------------
    // ShellState.content-constrained (state.slint:4114): window under the NPB
    // breakpoint AND a right panel (Queue / Lyrics) open. NOT a raw
    // `root.width < N` — the .slint calls that exact trigger out as the
    // regression it fixed (ArtistPageView.slint:166-172): at a normal window
    // the content area is already narrow WITHOUT any panel, so the sidebar
    // would never auto-open at all.
    readonly property bool contentConstrained:
        Window.width > 0 && Window.width < 1366
        && (QbzShell.queueOpen || QbzShell.lyricsOpen)
    // The .slint AUTO-collapses on a constrain edge and AUTO-opens when there
    // is room again, and re-applies the same rule on every artist change
    // (`changed net-cramped` / `changed net-nav-watch` at :175-180, plus
    // artist.rs `reset_network_sidebar`). The port opened FALSE and stayed
    // shut until the user found the button.
    property bool networkOpen: !contentConstrained
    onContentConstrainedChanged: networkOpen = !contentConstrained
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

    // --- Artist blacklist -------------------------------------------------
    // ONE source for the two surfaces the .slint drives off the same
    // ArtistState.is-blacklisted: the overflow-menu row label
    // (ArtistPageView.slint:565-567) and the hidden-artist banner (:595, :600).
    // artist_qt.rs does not seed the field yet (spec 03 C5 — main.rs:2653-2659
    // does it in the reference), so `artist.isBlacklisted` reads `undefined`
    // today and toggleState's `fallback === true` folds that to false; the
    // optimistic flip + the `blacklistChanged` settle below make the page
    // correct within a visit, and it becomes correct on ENTRY the moment the
    // seed lands, with no QML change.
    readonly property bool artistBlacklisted: toggleState("artistBlacklist", artist.isBlacklisted)
    function toggleBlacklist() {
        var aid = artist.id || ""
        if (aid === "")
            return
        // Optimistic flip first (main.rs:12777 `st.set_is_blacklisted(!was)`),
        // then the mutation; `blacklistChanged` settles it — or rolls it back
        // on a failed write (blacklist_qt.rs `artist_toggle`).
        setToggleState("artistBlacklist", !artistBlacklisted)
        QbzBlacklist.artistToggle(aid, artist.name || "")
    }

    readonly property var discoveryRows: {
        var out = []
        var rows = network.discovery || []
        for (var i = 0; i < rows.length; i++) {
            if (!dismissedDiscovery[rows[i].mbid]) out.push(rows[i])
        }
        return out
    }

    // ---- Loading staging (artist_qt.rs publishes in passes) --------------
    // The Qobuz page lands first; the Magazine stories and each MusicBrainz
    // sidebar section arrive later on their own flags. Every one of these is
    // ALSO gated on mbAvailable upstream, so with MusicBrainz off in Settings
    // (or no confident match) they are absent — placeholder included.
    readonly property bool primaryLoading: QbzArtist.artistLoading
                                           && (artist.topTracks || []).length === 0
    readonly property bool originPending: network.mbAvailable === true
                                          && network.originLoading === true
    readonly property bool relationshipsPending: network.mbAvailable === true
                                                 && network.relationshipsLoading === true
    readonly property bool discoveryPending: network.mbAvailable === true
                                             && network.discoveryLoading === true
                                             && root.discoveryRows.length === 0
    readonly property bool similarPending: similarArtists.length === 0 && QbzArtist.artistLoading
    readonly property bool storiesPending: artist.storiesLoading === true && stories.length === 0

    // ONE 900ms phase for every placeholder on the page (QbzSkeleton's COST
    // note: N placeholders, 1 timer). Stops dead when nothing is pending.
    Timer {
        id: skeletonPhase
        property bool on: false
        interval: 900
        repeat: true
        running: root.visible && (root.primaryLoading || root.originPending
                                  || root.relationshipsPending || root.discoveryPending
                                  || root.similarPending || root.storiesPending)
        onTriggered: on = !on
    }

    // JUMP TO tabs from the present sections (ArtistState.jump-tabs).
    // Built from the RAW document, never the filtered lists: artist.rs
    // builds jump_tabs ONCE at load (`build_jump_tabs`) and `filter_artist`
    // does not touch them, so the strip must not reshuffle per keystroke.
    readonly property var jumpTabs: {
        var tabs = []
        var rawTop = artist.topTracks || []
        var rawSections = artist.releaseSections || []
        var rawAppears = artist.appearsOn || []
        if ((artist.bio || "") !== "") tabs.push({ "id": "about", "label": QbzSession.tr("About", QbzSession.trRev) })
        if (rawTop.length > 0) tabs.push({ "id": "popular-tracks", "label": QbzSession.tr("Popular Tracks", QbzSession.trRev) })
        for (var i = 0; i < rawSections.length; i++) {
            if (rawSections[i].releaseType !== "other")
                tabs.push({ "id": rawSections[i].releaseType, "label": rawSections[i].title })
        }
        if (rawAppears.length > 0) tabs.push({ "id": "appears-on", "label": QbzSession.tr("Appears On", QbzSession.trRev) })
        return tabs
    }

    // Two blocks, not one: artwork is QbzLibrary's signal and the releases
    // pager is QbzArtist's. Retargeting a mixed block wholesale would
    // silently orphan the other half — QML resolves handlers lazily, so the
    // discography would just stop loading with nothing in the log.
    // Covers arrive ONE AT A TIME, and on a warm cache `sidebar_artwork_window`
    // emits the whole disk-hit set in a single synchronous loop (main.rs:391).
    // Rebinding `coverMap` per arrival is quadratic in the page: each arrival
    // copied the entire map and re-evaluated the cover binding of EVERY
    // mounted card, so an artist with 200 releases did ~40k binding
    // evaluations and 200 map copies during the frames the page is trying to
    // paint. Arrivals are coalesced into ONE rebind per frame — the same fix
    // LocalLibraryView carries, at O(n) instead of O(n²), with the covers
    // still appearing progressively (16ms granularity is invisible).
    property var _coverInbox: ({})
    Timer {
        id: coverFlush
        interval: 16
        repeat: false
        onTriggered: {
            var m = Object.assign({}, root.coverMap, root._coverInbox)
            root._coverInbox = ({})
            // A rebind needs a NEW object reference (same-ref assignment is
            // not a change in QML).
            root.coverMap = m
        }
    }
    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            root._coverInbox[key] = path
            if (!coverFlush.running) coverFlush.start()
        }
        // The SETTLED follow/heart state from Rust: the flipped value when the
        // write landed, the UNCHANGED one when it failed. Both the header
        // follow and the Popular Tracks / Appears On hearts write their
        // optimistic flip into localToggles and nothing used to correct it, so
        // a failed write stayed visibly wrong until the user left the page.
        // Key shape is `library_qt::feed_key` (`{kind}:{id}`) — which is
        // already the exact key the track rows use for their override.
        function onLibraryFavoriteChanged(key, value) {
            var aid = (artist && artist.id) ? artist.id : ""
            if (aid !== "" && key === "artist:" + aid)
                root.setToggleState("artist", value)
            else if (key.indexOf("track:") === 0)
                root.setToggleState(key, value)
        }
        // Header pin, same seam: the overflow-menu row flips `artistPin`
        // optimistically, and this settles it from the store (Slint does the
        // same for the open detail view: `st.set_pinned(pinned)` when its id
        // matches). The release CARDS need nothing here — each one listens to
        // this signal itself (cards/AlbumCard.qml).
        function onPinChanged(key, value) {
            var aid = (artist && artist.id) ? artist.id : ""
            if (aid !== "" && key === "artist:" + aid)
                root.setToggleState("artistPin", value)
        }
    }
    // Blacklist settle / rollback. `blacklistChanged` carries the state the
    // write actually produced — flipped on success, UNCHANGED on failure
    // (blacklist_qt.rs `artist_toggle`), which is exactly what main.rs:12799
    // does with its rollback branch. Also the cross-surface walk: unblocking
    // the same artist from the manager view while this page is mounted moves
    // the menu label and drops the banner. Same two-arg `{kind}:{id}` shape as
    // the two signals above; its own Connections block only because the signal
    // lives on a different singleton.
    Connections {
        target: QbzBlacklist
        function onBlacklistChanged(key, value) {
            var aid = (artist && artist.id) ? artist.id : ""
            if (aid !== "" && key === "artist:" + aid)
                root.setToggleState("artistBlacklist", value)
        }
    }

    Connections {
        target: QbzArtist
        function onReleaseSectionReady(releaseType, cardsJson, hasMore) {
            var cards = JSON.parse(cardsJson)
            // The document, not root.releaseSections: that one is a FILTERED
            // projection and may hand back fresh objects, so a push into it
            // would be dropped on the next re-evaluation.
            var sections = root.artist.releaseSections || []
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
    // (the raw document drives the dispatch; onArtistChanged above covers it)
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
        // …and re-applies the room rule (ArtistPageView.slint:178, artist.rs
        // `reset_network_sidebar`): a new artist re-opens the panel unless
        // the content area is constrained.
        networkOpen = !contentConstrained
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

    // THE RULE FOR THIS FUNCTION: every section of the page that binds
    // `root.coverMap` has to contribute its urls here, or its covers are
    // never requested and the section renders empty tiles forever — nothing
    // downstream reports the omission, because a missing key is
    // indistinguishable from a cover that has not landed yet.
    //
    // Three sections were missing and each one rendered blank:
    //   - Latest release (`artist.lastRelease`, the reported bug) — one card,
    //     and the only cover between Popular Tracks and the release grids;
    //   - Appears On (`artist.appearsOn`) — TrackRow covers, the same
    //     PopularTrackRow component the collected topTracks use, which is why
    //     the omission was easy to miss;
    //   - Playlists (`artist.playlists`) — the 200px rectangle covers of the
    //     horizontal strip.
    // Present and collected: the header portrait, Popular Tracks, every
    // release section (including the collapsed "Other"), and the Magazine
    // stories. The Network sidebar carries NO covers (ArtistSimilar and
    // MbDiscoveryJson have no artUrl — artist_qt.rs), so it contributes none.
    // The In-library tab has its own dispatcher, dispatchLibCovers().
    function dispatchCovers() {
        var urls = []
        if (artist.artUrl) urls.push(artist.artUrl)
        var i, j
        // RAW lists: a filtered-out card still needs its cover for when the
        // query is cleared, and this must not re-run per keystroke.
        var rawTop = artist.topTracks || []
        var rawAppears = artist.appearsOn || []
        var rawSections = artist.releaseSections || []
        var rawPlaylists = artist.playlists || []
        for (i = 0; i < rawTop.length; i++) if (rawTop[i].artUrl) urls.push(rawTop[i].artUrl)
        for (i = 0; i < rawAppears.length; i++)
            if (rawAppears[i].artUrl) urls.push(rawAppears[i].artUrl)
        if (artist.lastRelease && artist.lastRelease.artUrl)
            urls.push(artist.lastRelease.artUrl)
        for (i = 0; i < rawSections.length; i++)
            for (j = 0; j < (rawSections[i].cards || []).length; j++)
                if (rawSections[i].cards[j].artUrl) urls.push(rawSections[i].cards[j].artUrl)
        for (i = 0; i < rawPlaylists.length; i++)
            if (rawPlaylists[i].artUrl) urls.push(rawPlaylists[i].artUrl)
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
    //
    // COLUMN GEOMETRY: rows/TrackCols.qml, the same object rows/TrackRow.qml
    // and rows/TrackListHeader.qml read. This component is a FORK of the
    // shared row (POC-NOTE, pre-existing) and it had the full column set
    // re-typed as literals; they were numerically right, but a fork with its
    // own copy of the numbers is precisely how a table's columns drift. The
    // artist page draws no column header (neither does
    // artist/ArtistPageView.slint), so nothing is misaligned today — this
    // keeps it that way if the widths ever move.
    component PopularTrackRow: Rectangle {
        id: popRow
        property var row: ({})
        property int rowIndex: 0
        property bool showAlbum: true

        TrackCols { id: cols }

        readonly property bool isActive: QbzPlayer.npTrackId !== "" && QbzPlayer.npTrackId === row.id
        readonly property bool hovered: trArea.containsMouse || favArea.containsMouse || moreArea.containsMouse

        width: parent ? parent.width : 0
        height: 50
        radius: 8
        color: hovered ? root.rowHoverBg : (rowIndex % 2 === 1 ? "#07ffffff" : "transparent")

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
            anchors.leftMargin: cols.padH
            anchors.rightMargin: cols.padH
            spacing: cols.gap

            // Position number (artwork rows carry it separate from the cover).
            Text {
                visible: showAlbum
                width: cols.colNumber
                anchors.verticalCenter: parent.verticalCenter
                text: row.number
                color: theme.textMuted
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
            }
            // Cover with hover play overlay.
            Rectangle {
                width: showAlbum ? cols.colArt : cols.colNumber
                height: showAlbum ? cols.colArt : 28
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
                    // On the #000000 @ 0.6 artwork scrim above — dark under
                    // every theme.
                    tintName: "white"
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzPlayer.playArtistTrack(row.id)
                }
            }
            // Title + artist.
            //
            // This was `- 6 * 14` for the gaps in BOTH arms. With the album
            // column on there are nine visible cells, i.e. EIGHT gaps, so the
            // stretch column ran 28px long and dragged Album / Duration /
            // Quality / heart 28px right of where the shared TrackRow puts
            // them — the same class of defect as the header, inside a forked
            // row. `cols.titleWidth` counts the gaps from the arms.
            //
            // Arms: with the album column the leading cells are the 32px
            // number AND the 36px cover (artwork arm); without it the single
            // 32px cover IS the number cell.
            Column {
                width: cols.titleWidth(popRow.width, popRow.showAlbum, popRow.showAlbum,
                                       true, true, true)
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
                width: showAlbum ? cols.colAlbum : 0
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
                width: cols.colDuration
                anchors.verticalCenter: parent.verticalCenter
                text: row.duration
                color: theme.textMuted
                font.pixelSize: 13
                horizontalAlignment: Text.AlignHCenter
            }
            Text {
                width: cols.colQuality
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
                width: cols.colFavorite
                height: cols.colFavorite
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: favArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: parent.favorite ? "heart-filled" : "heart"
                    width: 16
                    height: 16
                    tintName: parent.favorite
                        ? "favorite"
                        : (favArea.containsMouse ? root.tintOnSurface : "muted")
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
                width: cols.colDownload
                height: cols.colDownload
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                QbzIcon { anchors.centerIn: parent; name: "cloud-download"; width: 16; height: 16; tintName: "muted" }
            }
            // ⋯ row menu. It used to be a hover-lit button with NO handler at
            // all — a control that renders and no-ops, which is the defect
            // class this round is closing.
            Rectangle {
                width: cols.colMenu
                height: cols.colMenu
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { anchors.centerIn: parent; name: "ellipsis"; width: 16; height: 16; tintName: moreArea.containsMouse ? root.tintOnSurface : "muted" }
                MouseArea {
                    id: moreArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { popMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
                }
            }
        }

        // primitives/TrackContextMenu.slint, in ITS order, restricted to the
        // entries whose seam is live at this call site.
        //
        // REUSE, and why it is not `rows/TrackRow.qml`: PopularTrackRow is a
        // pre-existing, documented fork (see the component header) and the
        // fork is load-bearing here — its heart reads the VIEW-level
        // optimistic store (`root.toggleState("track:" + id, …)`, shared with
        // the header and the library tab) while TrackRow owns a private
        // `favorite` property, and its play route is the artist-context
        // `playArtistTrack`. Swapping the component is a view rewrite, not a
        // menu fix. What IS reused is the menu surface itself:
        // `controls/CardMenu.qml`, the same primitive rows/TrackRow.qml opens.
        //
        // ABSENT, not dead (the same discipline TrackRow applies): the radio
        // pair, Share Qobuz link / Song.link, the offline-cache block and
        // Track info. The first two need bridge seams that do not exist; the
        // last two need the shared row's lazy Loaders (a TextEdit for the
        // clipboard, a TrackInfoModal), and duplicating those per artist row
        // is exactly the fork-drift this file already paid for once.
        CardMenu {
            id: popMenu
            menuWidth: 224
            entries: {
                var t = QbzSession.tr
                var r = QbzSession.trRev
                var fav = root.toggleState("track:" + popRow.row.id, popRow.row.isFavorite)
                var m = [
                    { "label": t("Play now", r), "icon": "play-fill", "action": "play" },
                    { "label": t("Play next", r), "icon": "list-start", "action": "next" },
                    { "label": t("Play later", r), "icon": "list-plus", "action": "later" },
                    { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
                    { "label": fav ? t("Remove from Library", r) : t("Add to Library", r),
                      "icon": fav ? "heart-filled" : "heart", "action": "favorite" },
                    { "label": t("Add to mixtape", r), "icon": "cassette-tape", "action": "mixtape" },
                    { "label": t("Add to playlist", r), "icon": "list-music", "action": "add-to-playlist" },
                ]
                if ((popRow.row.albumId || "") !== "")
                    m.push({ "label": t("Go to album", r), "icon": "disc-3", "action": "go-album" })
                if ((popRow.row.artistId || "") !== "")
                    m.push({ "label": t("Go to artist", r), "icon": "user", "action": "go-artist" })
                return m
            }
            onPicked: function (a) {
                var id = popRow.row.id
                if (a === "play") QbzPlayer.playArtistTrack(id)
                else if (a === "next") QbzPlayer.enqueueTrack(id, "next")
                else if (a === "later") QbzPlayer.enqueueTrack(id, "later")
                else if (a === "queue") QbzPlayer.enqueueTrack(id, "queue")
                else if (a === "favorite") {
                    root.setToggleState("track:" + id,
                        !root.toggleState("track:" + id, popRow.row.isFavorite))
                    QbzLibrary.libraryToggleFavorite("track", id)
                }
                else if (a === "add-to-playlist") QbzPlaylistPicker.openForTrack(id)
                else if (a === "mixtape") {
                    // The HOST builds the AddItem payload. SOURCE: every row
                    // this component draws is a Qobuz catalog track — the
                    // `/artist/page` `top_tracks` / `appears_on` lists, and
                    // the in-library tab's rows, which artist_qt maps from
                    // Qobuz `Track` values. There is no local artist page.
                    QbzMyQbzAdd.open(JSON.stringify([{
                        "itemType": "track", "source": "qobuz",
                        "sourceItemId": id, "title": popRow.row.title || "",
                        "subtitle": popRow.row.artist || "", "artworkUrl": "",
                        "year": null, "trackCount": null
                    }]))
                }
                else if (a === "go-album") QbzAlbum.openAlbum(popRow.row.albumId)
                else if (a === "go-artist") QbzArtist.openArtist(popRow.row.artistId)
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
        // Right press opens the SAME menu at the pointer — the invariant
        // controls/QbzContextMenu.qml:20-22 states. Declared last so it sits
        // on top, RIGHT-only so every left click still falls through.
        MouseArea {
            id: popRcArea
            anchors.fill: parent
            acceptedButtons: Qt.RightButton
            onClicked: function (mouse) { popMenu.openAtCursor(popRcArea, mouse.x, mouse.y) }
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
                tintName: slArea.containsMouse ? root.tintOnSurface : "muted"
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

    // Small 11px muted line — sub-group labels and the empty states inside
    // the sidebar sections. (The "Loading…" lines are now SidebarSkeleton.)
    component SidebarNote: Text {
        color: theme.textMuted
        font.pixelSize: 12
    }

    // Placeholder rows for a sidebar section still in flight — the shared
    // QbzSkeleton at the 28px pitch of SidebarLink, so the section holds its
    // band and nothing jumps when the real links land.
    //
    // `phase` is a property, not a file-scope id lookup: an inline `component`
    // does not see the enclosing document's ids (QbzSkeleton.qml's gotcha), so
    // the host passes the one shared timer in.
    component SidebarSkeleton: Column {
        id: sbSk
        property bool phase: false
        property int rows: 3
        // -28 = the section Column's left+right padding (see OriginRow).
        width: parent ? parent.width - 28 : 0
        spacing: 9
        Repeater {
            model: sbSk.rows
            delegate: QbzSkeleton {
                required property int index
                variant: "block"
                width: sbSk.width * (index % 2 === 0 ? 0.86 : 0.6)
                height: 13
                cellIndex: index
                phase: sbSk.phase
            }
        }
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
            // Per-section sort (ReleaseGrid.slint:70 QbzSelect). No seam:
            // `set-section-sort` persists per release_type and re-sorts
            // server-side, and neither exists on this bridge — so the control
            // is DIMMED and carries no MouseArea at all instead of offering a
            // pointer cursor over a menu that never opens.
            Rectangle {
                id: sortBtn
                width: sortRow.width
                height: 28
                radius: 5
                opacity: 0.4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
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
                    // The subtitle slot carries the YEAR on the artist page,
                    // not the artist: artist.rs `card_to_item` (:670-688)
                    // re-routes `year` through the card's `artist` field
                    // precisely so the shared card primitive stays unchanged
                    // ("the artist is redundant since we're already on their
                    // page"), and blanks artist_id so the line is inert.
                    artist: modelData.year
                    artistId: ""
                    genre: modelData.genre
                    year: modelData.year
                    qualityTier: modelData.qualityTier
                    artSource: root.coverMap[modelData.artUrl] || ""
                    // artist_qt `map_release` stamps the pin state on every
                    // release row; SectionRail is the only other reader and
                    // this page never mounts it, so the flag was published
                    // and dropped on the floor — the glyph lied on all four
                    // of this page's album grids. `artUrl` is the REMOTE url
                    // (coverMap is keyed BY it), which is what the pin
                    // payload must store.
                    isPinned: modelData.isPinned === true
                    artworkUrl: modelData.artUrl || ""
                    // artist_qt::map_release stamps this the same way it
                    // stamps the pin; false inverted the first click.
                    isFavorite: modelData.isFavorite === true
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
    // FULL WIDTH even with the sidebar open. The .slint reserves the 300px
    // inside the BODY ROW only (ArtistPageView.slint:1094) — the header and
    // the JUMP TO bar span the whole window so the gradient covers edge to
    // edge (:184-191). Narrowing the whole Flickable (the port's old
    // `anchors.rightMargin`) squeezed the portrait and the bio too.
    Flickable {
        id: flick
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        // Artwork-tinted header band — the SAME shared component AlbumView
        // mounts. First child = painted under the page; inside the Flickable
        // = scrolls with it (ArtistPageView.slint:213); full-bleed.
        HeaderGradient {
            x: 0
            y: 0
            width: flick.width
            // .slint:147 `atmo-height: page.y + body-row.y` — the band ends
            // exactly where the body begins, i.e. at the JUMP TO strip, so a
            // long bio pushes the fade down with no manual tuning.
            height: page.y + jumpBar.y
            tint: artist.headerColor || ""
            // Route A: the blurred field. Empty until the cover resolves, and the
            // flat tint stands in meanwhile (HeaderGradient handles the swap).
            atmosphere: artist.headerAtmosphere || ""
            active: root.headerAtmoOn
        }

        Column {
            id: page
            width: parent.width
            leftPadding: 32
            rightPadding: 32
            topPadding: 11
            bottomPadding: 100
            spacing: 0

            // Width available to the BODY sections: the page width less the
            // 32+32 padding, less the sidebar reservation while it is open
            // (.slint's empty 300px slot in the body row). The header and the
            // jump strip deliberately do NOT subtract it.
            readonly property real bodyWidth: width - 64 - (root.networkOpen ? 300 : 0)

            Item { width: 1; height: 22 }

            // --- Artist header skeleton ----------------------------------
            // Mounted on the primary flag, and the real header is hidden by
            // the same flag: opening artist B never renders a half-empty
            // header frame while B's document is in flight.
            Row {
                visible: root.primaryLoading
                width: parent.width - 64
                spacing: 32

                QbzSkeleton { variant: "circle"; width: 200; height: 200; phase: skeletonPhase.on }
                Column {
                    width: parent.width - 200 - 32
                    spacing: 12
                    Item { width: 1; height: 10 }
                    QbzSkeleton { variant: "block"; width: Math.min(360, parent.width); height: 30; cellIndex: 0; phase: skeletonPhase.on }
                    QbzSkeleton { variant: "block"; width: Math.min(520, parent.width); height: 14; cellIndex: 1; phase: skeletonPhase.on }
                    QbzSkeleton { variant: "block"; width: Math.min(440, parent.width); height: 14; cellIndex: 2; phase: skeletonPhase.on }
                    Item { width: 1; height: 14 }
                    Row {
                        spacing: 12
                        Repeater {
                            model: 4
                            delegate: QbzSkeleton {
                                required property int index
                                variant: "circle"
                                width: 44
                                height: 44
                                cellIndex: index
                                phase: skeletonPhase.on
                            }
                        }
                    }
                }
            }

            // --- Artist header ------------------------------------------
            Row {
                visible: !root.primaryLoading
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
                        color: root.hdrStrong
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }

                    Item { visible: (artist.bio || "") !== ""; width: 1; height: 12 }
                    Text {
                        visible: (artist.bio || "") !== ""
                        width: parent.width
                        text: artist.bioShort || ""
                        color: root.hdrBody
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
                    // Action row — ArtistPageView.slint:413-591. Four
                    // circles (NO Play: Popular Tracks carries its own), then
                    // a stretch, then the catalog/library toggle floated
                    // right. The palette arm follows the header backdrop
                    // (`on-surface: root.hdr-on-surface`, :417).
                    Row {
                        width: parent.width
                        spacing: 12
                        QbzCircleAction {
                            readonly property bool following: root.toggleState("artist", artist.isFollowing)
                            name: following ? "heart-filled" : "heart"
                            overlay: root.hdrOverlay
                            active: following
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: {
                                root.setToggleState("artist", !following)
                                QbzLibrary.libraryToggleFavorite("artist", artist.id)
                            }
                        }
                        // Radio — the .slint opens a QBZ-radio / Qobuz-radio
                        // dropdown; neither engine has a seam on this bridge,
                        // and the dropdown it used to open had two rows that
                        // just closed themselves. DIMMED and inert-by-
                        // declaration until an engine lands.
                        QbzCircleAction {
                            id: radioBtn
                            name: "radio"
                            overlay: root.hdrOverlay
                            btnEnabled: false
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        QbzCircleAction {
                            name: "element-connect"
                            overlay: root.hdrOverlay
                            active: root.networkOpen
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: root.networkOpen = !root.networkOpen
                        }
                        QbzCircleAction {
                            id: overflowBtn
                            name: "ellipsis"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) { overflowMenu.openAtCursor(overflowBtn, mouse.x, mouse.y) }
                        }
                        // Stretch (.slint:579 `Rectangle { horizontal-stretch: 1 }`).
                        // Clamped: at a narrow window an unclamped negative
                        // width silently reflows the whole row.
                        Item {
                            width: Math.max(0, parent.width - 4 * 32 - 4 * 12
                                               - (segTabs.visible ? segTabs.width + 12 : 0))
                            height: 1
                        }
                        // From catalog / In library — .slint:582 mounts the
                        // SHARED SegmentedTabBar; the port hand-rolled a copy
                        // of it here whose delegate walked the wrong parent
                        // chain (`parent.parent.modelData` on a Row, and the
                        // count chip read `active` off the wrong node), so the
                        // count badge never took its active colours. This is
                        // the shared control (controls/QbzTabBar.qml), which
                        // is that same SegmentedTabBar 1:1 — counts on, and
                        // the 2px accent underline the .slint's Segment draws
                        // for the active tab (:86-93).
                        QbzTabBar {
                            id: segTabs
                            visible: (artist.libraryCount || 0) > 0
                            anchors.verticalCenter: parent.verticalCenter
                            counts: true
                            underline: true
                            activeId: root.artistTab
                            tabs: [
                                { "id": "catalog", "label": QbzSession.tr("From catalog", QbzSession.trRev), "count": 0 },
                                { "id": "library", "label": QbzSession.tr("In library", QbzSession.trRev), "count": artist.libraryCount || 0 },
                            ]
                            onSelected: function (id) { root.artistTab = id }
                        }
                    }
                }
            }

            // --- Hidden-artist banner (ArtistPageView.slint:595-660) ------
            // Only when the CURRENTLY displayed artist is blacklisted. The page
            // stays fully navigable — a direct fetch-by-id is never blocked
            // (.slint:595-599) — so this is an unblock affordance, not a lock.
            // Sits between the header and the body row, exactly where the
            // .slint puts it (after the header block at :594, before the body
            // row), with the .slint's own 16px spacer (:600).
            // Built inline rather than through controls/WarningBanner.qml: that
            // control has no action slot, and this banner's whole point is the
            // right-hand "Show artist" button.
            Item { visible: root.artistBlacklisted; width: 1; height: 16 }
            Rectangle {
                id: hiddenBanner
                visible: root.artistBlacklisted
                width: parent.width - 64
                // .slint `height: banner-row.preferred-height` where the row is
                // a HorizontalLayout with padding 12 (:602) — so 24 plus the
                // tallest child: the 16px glyph, the wrapped copy, or the 28px
                // button (:604, :620, :639).
                height: visible ? 24 + Math.max(16, bannerCopy.implicitHeight, 28) : 0
                radius: 8
                // LITERALS, not theme tokens: the .slint hardcodes both
                // (:596-599). theme.warningBg / warningBorder are a different
                // amber (#fbbf24-based) and would not match the reference.
                color: "#eab3081a"
                border.width: 1
                border.color: "#eab3084d"

                // 16x16 blind-eye (:606-611). The .slint tints it with the
                // literal #eab308; QbzIcon.tintName is a CLOSED vocabulary of
                // names with no #eab308 bake, so "warning" (theme.warning
                // #fbbf24) is the nearest available and the only theme-following
                // amber — spec 03 C6's default decision.
                QbzIcon {
                    id: bannerGlyph
                    name: "blind-eye"
                    width: 16
                    height: 16
                    tintName: "warning"
                    anchors.left: parent.left
                    anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                }
                // "Show artist" button (:634-659): width = label + 20, height
                // 28, radius 6, hover fill surface-elevated else transparent,
                // label accent -> accentHover on hover, 13 / semibold.
                Rectangle {
                    id: bannerBtn
                    width: bannerBtnLabel.implicitWidth + 20
                    height: 28
                    radius: 6
                    color: bannerBtnArea.containsMouse ? theme.surfaceElevated : "transparent"
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        id: bannerBtnLabel
                        anchors.centerIn: parent
                        text: QbzSession.tr("Show artist", QbzSession.trRev)
                        color: bannerBtnArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: 13
                        font.weight: theme.weightSemibold
                    }
                    MouseArea {
                        id: bannerBtnArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        // Same seam as the menu row (:654-657) — one toggle.
                        onClicked: root.toggleBlacklist()
                    }
                }
                // Copy (:619-627): text-secondary, Typography.legal = 13,
                // word-wrap. The 10px gaps on both sides are the .slint's
                // HorizontalLayout spacing (:603).
                Text {
                    id: bannerCopy
                    anchors.left: bannerGlyph.right
                    anchors.leftMargin: 10
                    anchors.right: bannerBtn.left
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("This artist is hidden from discovery", QbzSession.trRev)
                    color: theme.textSecondary
                    font.pixelSize: 13
                    wrapMode: Text.WordWrap
                }
            }

            Item { width: 1; height: 20 }

            // --- JUMP TO bar ---------------------------------------------
            // The SHARED controls/QbzJumpNavBar (primitives/JumpNavBar.slint
            // 1:1). What the port drew by hand was the tab strip ONLY: no
            // "JUMP TO" caption, no bottom hairline, no search affordance,
            // 13px/medium type instead of the .slint's 15px/regular, and the
            // wrong three tab colours. padH 0 because this Column already
            // pads 32 — the .slint's own 32px padding lands the strip in the
            // same place.
            // POC-NOTE (unchanged): the bar scrolls with the page; the
            // .slint's sticky clamp (:1120-1126) is not ported.
            QbzJumpNavBar {
                id: jumpBar
                width: parent.width - 64
                padH: 0
                barBg: root.ambientOn ? "transparent" : theme.surfaceMain
                tabs: root.jumpTabs
                activeTabId: root.activeJumpTab
                onTabClicked: function (id) { root.scrollToSection(id) }
                onSearchEdited: function (text) { root.searchQuery = text }
            }

            // --- Primary placeholder --------------------------------------
            // Same flag the spinner used, now in the shape of what is coming:
            // the Popular Tracks heading plus 5 rows at the PopularTrackRow
            // 50px pitch (the preview count), so nothing shifts on arrival.
            Column {
                visible: root.primaryLoading
                width: parent.width - 64
                spacing: 0

                QbzSkeleton { variant: "block"; width: 190; height: 22; phase: skeletonPhase.on }
                Item { width: 1; height: 18 }
                Repeater {
                    model: root.preview
                    delegate: Item {
                        required property int index
                        width: parent ? parent.width : 0
                        height: 50
                        QbzSkeleton {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.leftMargin: 12
                            anchors.rightMargin: 24
                            height: 40
                            variant: "row"
                            cellIndex: index
                            phase: skeletonPhase.on
                        }
                    }
                }
            }

            // ================= Catalog tab ================================
            Column {
                id: sectionAnchors
                visible: root.artistTab === "catalog" && !QbzArtist.artistLoading
                // Body width: yields the 300px the sidebar overlay occupies
                // (.slint's empty reservation slot, :1094) so the content is
                // never painted under the panel.
                width: page.bodyWidth
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
                    // ArtistPageView.slint:732-737 mounts the SHARED
                    // CircleAction here — `primary: true` plus an explicit
                    // `on-surface: true` with the .slint's own reason on the
                    // line above it: "Plain page background (below the header
                    // divider) — theme-aware variant so it reads on light
                    // themes." The port hand-rolled a 44px accent disc
                    // instead, which duplicated the control AND bypassed that
                    // arm. `overlay` defaults false = the on-surface arm.
                    QbzCircleAction {
                        primary: true
                        name: "play-fill"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.playArtistTop(false)
                    }
                    // Multi-select has no seam on this bridge. Rendered but
                    // DIMMED and click-gated (CircleAction's own `enabled`
                    // treatment) rather than a live button that does nothing —
                    // same call as the header's Radio / Mixtape / Info.
                    QbzCircleAction {
                        name: "square-check-big"
                        btnEnabled: false
                        anchors.verticalCenter: parent.verticalCenter
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
                        // year in the subtitle slot — card_to_item again
                        // (artist.rs:784 maps last_release through it too).
                        artist: artist.lastRelease ? artist.lastRelease.year : ""
                        artistId: ""
                        genre: artist.lastRelease ? artist.lastRelease.genre : ""
                        year: artist.lastRelease ? artist.lastRelease.year : ""
                        qualityTier: artist.lastRelease ? artist.lastRelease.qualityTier : ""
                        artSource: artist.lastRelease ? (root.coverMap[artist.lastRelease.artUrl] || "") : ""
                        // Same row shape as the release grids (map_release).
                        isPinned: artist.lastRelease ? artist.lastRelease.isPinned === true : false
                        artworkUrl: artist.lastRelease ? (artist.lastRelease.artUrl || "") : ""
                        isFavorite: artist.lastRelease ? artist.lastRelease.isFavorite === true : false
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
                                    // year in the subtitle slot (card_to_item)
                                    artist: modelData.year
                                    artistId: ""
                                    genre: modelData.genre
                                    year: modelData.year
                                    qualityTier: modelData.qualityTier
                                    artSource: root.coverMap[modelData.artUrl] || ""
                                    isPinned: modelData.isPinned === true
                                    artworkUrl: modelData.artUrl || ""
                                    isFavorite: modelData.isFavorite === true
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
                width: page.bodyWidth
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
                    visible: libraryTab.libTracks.length > 0
                    text: QbzSession.tr("Tracks", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                }
                Item { visible: libraryTab.libTracks.length > 0; width: 1; height: 10 }
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
                            // These rows are LIBRARY feed items (FeedItem),
                            // not release cards: the remote url is
                            // `imageUrl`, and the pin state rides the same
                            // row (library_qt `map_album`).
                            isPinned: modelData.isPinned === true
                            artworkUrl: modelData.imageUrl || ""
                            isFavorite: modelData.isFavorite
                        }
                    }
                }
            }
        }

    }

    // Gutter scrollbar. A SIBLING of the Flickable, not a child: anything
    // declared inside a Flickable lands in its contentItem and scrolls away
    // with the page. Hidden while the network panel is open — that panel pins
    // to the same right edge and carries its own scroll
    // (ArtistPageView.slint:1157).
    QbzScrollBar {
        visible: !root.networkOpen
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: flick
    }

    // Library feed access (the phase-5 document, parsed in LibraryView).
    function libraryFeed() {
        return JSON.parse(QbzLibrary.libraryJson)
    }

    // --- Network sidebar (300px, surface-card + 1px left border) ---------
    // BOUNDED, not full-height. ArtistPageView.slint:1178-1195:
    //   natural-top = the live viewport y of the body row + the JUMP TO bar
    //                 height  (i.e. the panel starts under the strip, never
    //                 beside the portrait)
    //   y           = max(natural-top, sticky-top=44)  — it rides the scroll
    //                 down and then parks 44px from the top
    //   height      = root.height - y                 — always flush with the
    //                 bottom, so no gap appears once it is parked
    // The port ran it top-to-bottom of the whole view, which put the panel
    // alongside the header and made it read as app chrome instead of a page
    // panel.
    Rectangle {
        id: netPanel
        // Viewport-relative y of the strip's bottom edge. `jumpBar.y` is
        // content coords, so subtracting the scroll offset gives the live
        // viewport position — the same arithmetic the .slint does with
        // absolute-position.
        readonly property real naturalTop:
            page.y + jumpBar.y + jumpBar.height - flick.contentY
        readonly property real stickyTop: 44

        anchors.right: parent.right
        y: Math.max(naturalTop, stickyTop)
        height: Math.max(0, root.height - y)
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
                        tintName: netCloseArea.containsMouse ? root.tintOnSurface : "muted"
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
                        SidebarSkeleton {
                            visible: root.originPending
                            rows: 2
                            phase: skeletonPhase.on
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
                        SidebarSkeleton {
                            visible: root.similarPending
                            rows: 4
                            phase: skeletonPhase.on
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
                        SidebarSkeleton {
                            visible: root.relationshipsPending
                            rows: 3
                            phase: skeletonPhase.on
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
                        SidebarSkeleton {
                            visible: root.discoveryPending
                            rows: 4
                            phase: skeletonPhase.on
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
                                        tintName: dismissArea.containsMouse ? root.tintOnSurface : "muted"
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

                    // Story teasers are fetched after the page (Qobuz
                    // editorial): two card placeholders in the teaser shape
                    // while they resolve. Resolved-to-nothing keeps the
                    // explicit empty line below — this is a TAB body, where a
                    // blank panel would read as broken.
                    Column {
                        visible: root.storiesPending
                        width: magBody.width - 24
                        spacing: 12
                        Repeater {
                            model: 2
                            delegate: Column {
                                required property int index
                                width: parent ? parent.width : 0
                                spacing: 6
                                QbzSkeleton {
                                    variant: "block"
                                    width: parent.width
                                    height: parent.width
                                    blockRadius: 6
                                    cellIndex: index
                                    phase: skeletonPhase.on
                                }
                                QbzSkeleton { variant: "block"; width: parent.width * 0.82; height: 14; cellIndex: index; phase: skeletonPhase.on }
                                QbzSkeleton { variant: "block"; width: parent.width * 0.45; height: 11; cellIndex: index; phase: skeletonPhase.on }
                            }
                        }
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

    // --- ⋯ overflow menu ---------------------------------------------------
    QbzContextMenu {
        id: overflowMenu
        menuWidth: 224
            Repeater {
                // `live: false` = no seam on this bridge. The .slint greys
                // its own unavailable rows the same way (Artist Scene is
                // `enabled: NetworkSidebarState.mb-available`,
                // ArtistPageView.slint:523) — dimmed, no hover, no click,
                // and the menu keeps its shape.
                model: [
                    { "label": QbzSession.tr("Create Artist Collection", QbzSession.trRev), "icon": "library-big", "action": "disco", "live": true },
                    { "label": QbzSession.tr("Artist Scene", QbzSession.trRev), "icon": "map-pin", "action": "stub", "live": false },
                    { "label": QbzSession.tr("Share", QbzSession.trRev), "icon": "link", "action": "stub", "live": false },
                    { "label": root.toggleState("artistPin", artist.isPinned) ? QbzSession.tr("Unpin", QbzSession.trRev) : QbzSession.tr("Pin", QbzSession.trRev), "icon": root.toggleState("artistPin", artist.isPinned) ? "pin-filled" : "pin", "action": "pin", "live": true },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    opacity: modelData.live ? 1.0 : 0.4
                    color: (modelData.live && omiArea.containsMouse) ? theme.surfaceHover : "transparent"
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
                        enabled: modelData.live
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            overflowMenu.close()
                            if (modelData.action === "pin") {
                                root.setToggleState("artistPin", !root.toggleState("artistPin", artist.isPinned))
                                QbzLibrary.togglePin("artist", artist.id, artist.name, "", artist.artUrl)
                            } else if (modelData.action === "disco") {
                                // Discography Builder — ArtistPageView.slint
                                // :505-511 `media-action("artist", id,
                                // "build-collection")`. This is the ONLY route
                                // to it; the nav flyout has no builder entry.
                                QbzDisco.open(artist.id)
                            }
                        }
                    }
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            // ArtistPageView.slint:557-572 — the 1px border-subtle separator
            // above, then the LAST item, whose label flips
            // "Show artist" / "Blacklist artist" on ArtistState.is-blacklisted.
            // LIVE since QbzBlacklist landed (`artistToggle(id, name)`); it was
            // dimmed-and-inert only while the bridge had no invokable.
            Rectangle {
                width: parent.width
                height: 33
                radius: 5
                color: blkArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    spacing: 8
                    QbzIcon { name: "blind-eye"; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    Text {
                        height: parent.height
                        width: parent.width - 23
                        text: root.artistBlacklisted
                            ? QbzSession.tr("Show artist", QbzSession.trRev)
                            : QbzSession.tr("Blacklist artist", QbzSession.trRev)
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                MouseArea {
                    id: blkArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        overflowMenu.close()
                        root.toggleBlacklist()
                    }
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
                            else if (a === "playlist-all") {
                                // ArtistPageView.slint:797-802
                                // `top-tracks-menu-action("playlist-all")` —
                                // the picker over the section's tracks. `root
                                // .topTracks` is the FILTERED list, which is
                                // what `ArtistState.top-tracks` holds too
                                // (artist.rs filters into the state, the view
                                // never sees the raw list). Every row here is
                                // a Qobuz catalog track (`/artist/page`
                                // top_tracks), so the catalog arm is correct.
                                var ids = []
                                for (var i = 0; i < root.topTracks.length; i++)
                                    ids.push(String(root.topTracks[i].id))
                                if (ids.length > 0)
                                    QbzPlaylistPicker.openForTracks(JSON.stringify(ids))
                            }
                        }
                    }
                }
            }
        }
}
