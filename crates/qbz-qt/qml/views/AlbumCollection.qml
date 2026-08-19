// AlbumCollection — QML port of discover/AlbumCollectionView.slint: the
// rendered album collection (flat grid, flat list, or group-by sections),
// WITHOUT the surrounding toolbar / loading / empty / load-more states. The
// host owns those, exactly as in the .slint, so the column math and the
// list header+rows live in one place and are shared by Discover Browse,
// Label Releases and the two play-history pages.
//
// (views/local/LocalAlbumCollection.qml is the LOCAL twin: same idea, but
// its rows are LocalAlbumRow and it is coupled to LocalLibraryView for the
// skeleton pulse and the per-item artwork gate. Catalog pages have neither,
// so this is the catalog-side collection rather than a fifth arm bolted onto
// that one.)
//
// VIEW MODE: the flat grid mounts on `viewMode !== "list"`. The .slint tests
// `== "grid"` (:294) and therefore renders NOTHING for an empty view-mode —
// a trap the Rust seeding avoids anyway, but there is no reason to reproduce
// a blank page here.
//
// WINDOWING (flat grid only, opt-in via `flick`): only cards within ~one
// viewport of the visible band mount a real AlbumCard; the rest keep their
// footprint as bare Items, so the grid height — and therefore the scroll
// geometry — never changes. The band is SAMPLED by a timer rather than bound
// to `flick.contentY`: a direct binding re-evaluates every delegate on every
// scroll frame, which is the O(n)-per-frame cost the .slint's own sampler
// exists to avoid (see its "POST-LAYOUT SNAPSHOT SAMPLING" note). Grouped
// sections are never windowed — same call the .slint makes (:265-277).
//
// TAIL FADE (opt-in via armTailFade(), see the block below): the arrival half
// of the Load-more round. OFF unless a host arms it, so the two hosts that do
// not (Discover Browse, Play History) are bit-for-bit unchanged.

import QtQuick
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../theme"

Column {
    id: root

    /// Flat list of home_qt::HomeCard rows.
    property var albums: []
    /// [{ title, albums: [...] }] — only read while `isGrouped`.
    property var grouped: []
    property bool isGrouped: false
    /// "grid" | "list".
    property string viewMode: "grid"
    property int cardWidth: 200
    property int cardHeight: 266
    property int cardGap: 24
    property int listRowGap: 4
    /// Render the "{} plays" line (Most Played Albums only — AlbumCard.slint
    /// :508 shows it when the card carries a non-zero count, and every other
    /// surface publishes 0).
    property bool showPlays: false

    /// The scrolling host. Set it to enable grid windowing; leave null and
    /// every card mounts (the right choice for a bounded set like the 24-item
    /// play history).
    property Flickable flick: null
    /// Approximate y of this collection inside the host's content — the
    /// .slint's `content-offset`.
    property real contentOffset: 0

    QbzTheme { id: theme }

    width: parent ? parent.width : 0
    spacing: 0

    // --- Windowing band (row indices), sampled ---------------------------
    property int bandFirst: 0
    property int bandLast: 9999
    readonly property bool windowed: flick !== null

    function sampleBand() {
        if (!root.windowed || flatGrid.columns <= 0)
            return
        var pitch = root.cardHeight + root.cardGap
        var top = root.flick.contentY - root.contentOffset
        var h = root.flick.height
        // One viewport of prefetch on each side.
        root.bandFirst = Math.max(0, Math.floor((top - h) / pitch))
        root.bandLast = Math.max(0, Math.ceil((top + 2 * h) / pitch))
    }
    Timer {
        interval: 80
        repeat: true
        running: root.windowed && root.visible
        onTriggered: {
            root.sampleBand()
            root.reportArtWindow()
        }
    }
    Component.onCompleted: {
        root.sampleBand()
        root.reportArtWindow()
    }

    // --- Windowed artwork -------------------------------------------------
    //
    // The delegates were already windowed; their COVERS were not. Rust fetched
    // every missing cover of the whole page in one batch and then republished
    // the entire document to attach the paths (`browse_qt::refresh_*_art`), so
    // on a View-all page nothing had artwork until everything did — and the
    // republish rebuilt every delegate on arrival. That is the same shape
    // Library > All was fixed out of: ask for what is near the viewport, and
    // take the answers ONE KEY AT A TIME so the model is never re-handed.
    //
    // Purely ADDITIVE for the hosts that do not window (they pass no `flick`):
    // nothing reports, `artMap` stays empty, and `artOf` falls through to the
    // `artPath` those pages already publish.
    //
    // `QbzShell.sidebarArtworkWindow` is the existing generic seam — it
    // resolves what is already on disk, downloads the rest and emits
    // `libraryArtworkReady` per url. It classifies local paths too, so the
    // local tabs that host this collection are served by the same call.
    property var artMap: ({})
    /// Urls already handed to the bridge. A NON-notifying holder on purpose:
    /// the sampler runs every 80 ms, and without this each unresolved cover
    /// would be re-dispatched twelve times a second until it landed.
    readonly property var _artAsked: ({ seen: ({}) })

    function artOf(m) {
        if (!m)
            return ""
        var u = m.artUrl || ""
        if (u !== "" && root.artMap[u])
            return root.artMap[u]
        return m.artPath || ""
    }

    function reportArtWindow() {
        if (!root.windowed || flatGrid.columns <= 0)
            return
        var cols = flatGrid.columns
        var lo = Math.max(0, root.bandFirst * cols)
        var hi = Math.min(root.albums.length - 1, (root.bandLast + 1) * cols - 1)
        var pending = []
        var asked = root._artAsked.seen
        for (var i = lo; i <= hi; i++) {
            var a = root.albums[i]
            if (!a)
                continue
            var u = a.artUrl || ""
            // Already on disk at publish time, already resolved, or already
            // asked for — all three mean there is nothing to request.
            if (u === "" || (a.artPath || "") !== "" || root.artMap[u] || asked[u] === true)
                continue
            asked[u] = true
            pending.push(u)
        }
        if (pending.length > 0)
            QbzShell.sidebarArtworkWindow(JSON.stringify(pending))
    }

    Connections {
        target: QbzLibrary
        // Shared with every other consumer of this signal — ignore keys that
        // are not ours.
        function onLibraryArtworkReady(key, path) {
            if (root.artMap[key] === path || root._artAsked.seen[key] !== true)
                return
            var m = root.artMap
            m[key] = path
            // Rebinding needs a NEW object reference; a same-ref assignment is
            // not a change in QML.
            root.artMap = Object.assign({}, m)
        }
    }

    // --- Tail fade: the appended page APPEARS, it does not pop -------------
    // The owner's second ask for the Load-more round (2026-08-02): "que la
    // aparicion de lo que se cargue, sea smooth". controls/QbzLoadMore.qml
    // owns the WAITING half (the skeleton under the button); the ARRIVAL half
    // can only live here — only the collection knows which of its delegates
    // are new. A host asks for it by calling armTailFade() immediately BEFORE
    // the bridge call that fetches the next page.
    //
    // WHY AN ID SET AND NOT "index >= the old count": this collection is
    // SORTED by its host (LabelReleasesView's sort control does newest /
    // oldest / title / artist, src/label_qt.rs::sort_cards). Only under
    // "newest" does a fetched page land as a tail; under any other order the
    // new albums INTERLEAVE, and an index threshold would then re-fade albums
    // that never moved — the very flicker this exists to avoid. Membership in
    // the pre-fetch id set is the honest question, and it costs one JS object
    // of N keys per click. It also lets GROUPED mode fade correctly, where
    // there is no flat index at all: a fetched album is folded into whichever
    // artist section it belongs to, anywhere in the page.
    //
    // WHY BOTH LISTS: `visible` and `grouped` are mutually exclusive in Rust —
    // derive_releases returns `Vec::new()` for the flat list as soon as
    // group-by is on (label_qt.rs:923). Snapshotting only `albums` would
    // capture an EMPTY set in grouped mode and every card on screen would read
    // as "new".
    //
    // INERT BY DEFAULT: a host that never arms leaves `_tailSeen` null, every
    // delegate's opacity short-circuits to 1.0, `_tailReveal` is never read
    // (so it is never even a binding dependency) and no animation exists.
    property var _tailSeen: null
    property real _tailReveal: 1.0

    /// Host-supplied identity of the page on screen (a label id, an artist
    /// id…). Changing it disarms a pending fade, so a click that never landed
    /// cannot colour the NEXT page's first paint. "" = the hosts that do not
    /// use the fade at all.
    property string collectionKey: ""
    onCollectionKeyChanged: root.clearTailFade()

    function _addIds(seen, list) {
        for (var i = 0; i < list.length; i++) {
            var a = list[i]
            if (a && a.id !== undefined && a.id !== null && a.id !== "")
                seen[a.id] = true
        }
    }

    /// Snapshot what is on screen NOW. Call it BEFORE the bridge call: the
    /// bridge may republish synchronously.
    function armTailFade() {
        var seen = {}
        root._addIds(seen, root.albums)
        for (var g = 0; g < root.grouped.length; g++)
            root._addIds(seen, root.grouped[g].albums || [])
        // Arming ALSO parks the reveal at 0. A delegate for a new id can be
        // built by a Repeater's model binding BEFORE onAlbumsChanged runs —
        // both hang off the same change signal and the order between them is
        // not ours to choose — and it must never be painted at full opacity
        // for one frame before the fade starts.
        root._tailReveal = 0.0
        root._tailSeen = seen
    }

    /// Disarm without fading.
    function clearTailFade() {
        root._tailSeen = null
        root._tailReveal = 1.0
    }

    /// 1.0 for everything the arm already saw — and for every host that never
    /// armed, and for a card with no id at all (an unknown id must fail
    /// OPAQUE, never invisible).
    function tailOpacity(id) {
        if (!root._tailSeen || id === undefined || id === null || id === "")
            return 1.0
        return root._tailSeen[id] === true ? 1.0 : root._tailReveal
    }

    function _hasFreshId(list) {
        for (var i = 0; i < list.length; i++) {
            var a = list[i]
            if (a && a.id !== undefined && a.id !== null && a.id !== ""
                    && root._tailSeen[a.id] !== true)
                return true
        }
        return false
    }

    function _maybeStartTailFade() {
        if (!root._tailSeen)
            return
        var fresh = root._hasFreshId(root.albums)
        for (var g = 0; !fresh && g < root.grouped.length; g++)
            fresh = root._hasFreshId(root.grouped[g].albums || [])
        // A republish with no new id — an artwork batch, a favourite toggle, a
        // sort flip, a group-by flip — is NOT the page landing. Stay armed and
        // stay silent; re-fading what is already on screen is the regression.
        if (fresh)
            tailFade.restart()
    }

    onAlbumsChanged: {
        root._maybeStartTailFade()
        // A new page appended -> new covers to ask for. ONE handler per
        // signal: declaring a second `onAlbumsChanged` is a hard qmlcachegen
        // error ("Property value set multiple times"), not a silent last-wins.
        root.reportArtWindow()
    }
    onGroupedChanged: root._maybeStartTailFade()

    NumberAnimation {
        id: tailFade
        target: root
        property: "_tailReveal"
        from: 0.0
        to: 1.0
        // 220ms / OutCubic is the content half of the Load-more round (the
        // skeleton half uses the tree's 180ms — QbzSkeleton.qml:216).
        duration: 220
        easing.type: Easing.OutCubic
        // Disarm on the way out, so the NEXT republish finds no arm at all.
        // `finished` is emitted only on a natural end — a restart() stops the
        // animation without it, which is exactly what we want mid-flight.
        onFinished: root.clearTailFade()
    }

    // One group-by section's card grid (FULL mounts — see the header note).
    // Declared before its use for readability; QML registers inline
    // components document-wide either way.
    component SectionGrid: Item {
        id: sg
        property var items: []
        /// Tail fade, PASSED IN rather than read off `root`: an inline
        /// `component` does not share the document's scope the way a plain
        /// Component does (controls/QbzSkeleton.qml:269-270 spells this out),
        /// so the mount site — which is in file scope — hands it the two
        /// values. Null / 1.0 = no fade, which is every other host.
        property var seenIds: null
        property real reveal: 1.0
        width: parent ? parent.width : 0
        readonly property int columns: Math.max(
            1, Math.floor((width + root.cardGap) / (root.cardWidth + root.cardGap)))
        readonly property int rows: Math.ceil(sg.items.length / sg.columns)
        height: sg.rows > 0
            ? sg.rows * root.cardHeight + (sg.rows - 1) * root.cardGap
            : 0

        Repeater {
            model: sg.items
            delegate: Item {
                id: gcell
                required property var modelData
                required property int index
                x: (gcell.index % sg.columns) * (root.cardWidth + root.cardGap)
                y: Math.floor(gcell.index / sg.columns) * (root.cardHeight + root.cardGap)
                width: root.cardWidth
                height: root.cardHeight
                // Same rule as the flat grid, expressed with the passed-in
                // pair (see `seenIds`): an album the arm already saw is
                // opaque, so only a fetched one fades.
                opacity: (sg.seenIds && gcell.modelData && gcell.modelData.id
                          && sg.seenIds[gcell.modelData.id] !== true)
                    ? sg.reveal : 1.0
                AlbumCard {
                    albumId: gcell.modelData.id
                    title: gcell.modelData.title
                    artist: gcell.modelData.artist
                    artistId: gcell.modelData.artistId
                    genre: gcell.modelData.genre
                    year: gcell.modelData.year
                    qualityTier: gcell.modelData.qualityTier
                    ribbon: gcell.modelData.ribbon || ""
                    ribbonKind: gcell.modelData.ribbonKind || ""
                    artSource: root.artOf(gcell.modelData)
                    isPinned: gcell.modelData.isPinned === true
                    // Snapshot url the pin payload persists (artPath is the
                    // local cache path — see AlbumCard.artworkUrl).
                    artworkUrl: gcell.modelData.artUrl || ""
                    // Stamped on the row (AlbumCardData / HomeCard); false
                    // made the glyph lie and inverted the first click.
                    isFavorite: gcell.modelData.isFavorite === true
                }
            }
        }
    }

    // --- Grouped sections -------------------------------------------------
    Column {
        visible: root.isGrouped && root.grouped.length > 0
        width: parent.width
        spacing: 0

        // In list mode the column header sits ONCE above all sections.
        AlbumListHeader { visible: root.viewMode === "list" }

        Repeater {
            model: root.isGrouped ? root.grouped : []
            delegate: Column {
                id: sectionCol
                required property var modelData
                required property int index
                width: root.width
                spacing: 12

                Item { visible: sectionCol.index > 0; width: 1; height: 24 }
                Text {
                    text: sectionCol.modelData.title || ""
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                }
                // List arm.
                Column {
                    visible: root.viewMode === "list"
                    width: parent.width
                    spacing: root.listRowGap
                    Repeater {
                        model: root.viewMode === "list" ? (sectionCol.modelData.albums || []) : []
                        delegate: AlbumListRow {
                            required property var modelData
                            required property int index
                            item: modelData
                            rowIndex: index
                            // The row prefers an explicit source and falls back
                            // to `item.artPath`, so handing it the windowed map
                            // costs nothing when the map is empty.
                            artSource: root.artOf(modelData)
                            // Plain Component, file scope — `root` resolves
                            // here, unlike inside SectionGrid.
                            opacity: root.tailOpacity(modelData ? modelData.id : "")
                        }
                    }
                }
                // Grid arm.
                SectionGrid {
                    visible: root.viewMode !== "list"
                    items: sectionCol.modelData.albums || []
                    seenIds: root._tailSeen
                    reveal: root._tailReveal
                }
            }
        }
    }

    // --- Flat list --------------------------------------------------------
    Column {
        visible: !root.isGrouped && root.viewMode === "list" && root.albums.length > 0
        width: parent.width
        spacing: root.listRowGap

        AlbumListHeader { }
        Repeater {
            model: (!root.isGrouped && root.viewMode === "list") ? root.albums : []
            delegate: AlbumListRow {
                required property var modelData
                required property int index
                item: modelData
                artSource: root.artOf(modelData)
                rowIndex: index
                opacity: root.tailOpacity(modelData ? modelData.id : "")
            }
        }
    }

    // --- Flat grid (windowed) --------------------------------------------
    Item {
        id: flatGrid
        visible: !root.isGrouped && root.viewMode !== "list" && root.albums.length > 0
        width: parent.width
        readonly property int columns: Math.max(
            1, Math.floor((width + root.cardGap) / (root.cardWidth + root.cardGap)))
        readonly property int rows: Math.ceil(root.albums.length / flatGrid.columns)
        height: flatGrid.rows > 0
            ? flatGrid.rows * root.cardHeight + (flatGrid.rows - 1) * root.cardGap
            : 0

        Repeater {
            model: (!root.isGrouped && root.viewMode !== "list") ? root.albums : []
            delegate: Item {
                id: cell
                required property var modelData
                required property int index
                readonly property int rowIndex: Math.floor(cell.index / flatGrid.columns)
                x: (cell.index % flatGrid.columns) * (root.cardWidth + root.cardGap)
                y: cell.rowIndex * (root.cardHeight + root.cardGap)
                width: root.cardWidth
                height: root.cardHeight
                // Tail fade (see the block at the top). A LIVE binding, never
                // a Component.onCompleted write: this delegate is destroyed
                // and rebuilt on every republish, and the binding gives the
                // rebuilt cell the right value at creation instead of a frame
                // at the wrong one. Everything the arm saw — and everything,
                // on every host that never arms — reads 1.0 and never even
                // depends on `_tailReveal`.
                opacity: root.tailOpacity(cell.modelData ? cell.modelData.id : "")

                // Component declared in the DELEGATE scope so `modelData`
                // resolves (the PinnedRail dispatch pattern).
                Component {
                    id: cardComp
                    AlbumCard {
                        albumId: cell.modelData.id
                        title: cell.modelData.title
                        artist: cell.modelData.artist
                        artistId: cell.modelData.artistId
                        genre: cell.modelData.genre
                        year: cell.modelData.year
                        qualityTier: cell.modelData.qualityTier
                        ribbon: cell.modelData.ribbon || ""
                        ribbonKind: cell.modelData.ribbonKind || ""
                        artSource: root.artOf(cell.modelData)
                        isPinned: cell.modelData.isPinned === true
                        artworkUrl: cell.modelData.artUrl || ""
                        plays: root.showPlays ? (cell.modelData.plays || 0) : 0
                        isFavorite: cell.modelData.isFavorite === true
                    }
                }
                Loader {
                    anchors.fill: parent
                    active: !root.windowed
                        || (cell.rowIndex >= root.bandFirst && cell.rowIndex <= root.bandLast)
                    sourceComponent: cardComp
                }
            }
        }
    }
}
