// Search results view — the QML port of search/SearchResultsView.slint
// (phase 15). One JSON document (QbzBridge.searchJson, search_qt.rs
// SearchPageDoc: query/tab/loading/filterIndex + the four category lists,
// totals, the most-popular hero, the carousel-only artists list).
//
// Layout (1:1): 56px toolbar ("Search" title + the searchType filter
// radios on the right), the five-tab strip (All / Albums / Tracks /
// Artists / Playlists), the scrollable body: All = Most-popular hero +
// Artists carousel + Albums carousel + Tracks preview (6) + Playlists
// carousel; per-type tabs = full grid/list + "Load more (n / total)".
//
// POC-NOTEs:
// - Card click actions: albums/artists navigate; tracks play; playlists
//   open (openPlaylist is wired; the stale "inert" note was dropped in
//   phase 21, when every card surface moved to the shared qml/ cards).
// - The most-popular track hero stays a DISTINCT component
//   (SearchTrackHero below): in Slint the search hero is
//   primitives/SearchTrackHero.slint, NOT discover/TrackCard.slint —
//   the POC variant is 200x246 (centered play, quality as text) to match
//   the ArtistGridCard hero slot it shares the row with.
// - The Slint's windowed grid virtualization is not replicated (page size
//   is 20 — the whole set mounts).
//
// LOADING (a deliberate ADDITION over the Slint's bare LoadingSpinner): the
// view mounts QbzSkeleton placeholders in the shape of the tab that is
// coming, plus a per-card cover placeholder that clears when THAT card's
// cover lands. See the "skeleton plumbing" block below for the cost.
//
// LOAD MORE: the four per-tab buttons are the shared controls/QbzLoadMore.qml
// on its BORDERED arm (that arm IS this file's old `LoadMoreButton`, pixel for
// pixel — see that file's header, item c). Two things moved with it:
//   - the appended-page placeholder now hangs BELOW its button instead of
//     above it. Each tab had a standalone QbzSkeleton sitting on top of the
//     button; that is the wrong side, because the page being fetched is
//     appended UNDER the content already on screen and the button is the
//     bottom of that content. The knobs (pitch, settleMs) came across.
//   - the appended DELEGATES fade in — the other half of what the owner asked
//     for ("que la aparicion de lo que se cargue, sea smooth"). Only this file
//     can do that half: only the host knows which delegates are new. See the
//     `fadeAt` block below.
//
// SIZE: this file is over the 500-line guideline (it already was at 638
// before the skeleton work). It is NOT split because every section is one
// tab of one document and the split would be arbitrary; the skeleton
// additions are the composite mounts, which are ~8 lines each precisely to
// keep it from growing further.

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
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzBridge.searchJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var albums: doc.albums || []
    readonly property var tracks: doc.tracks || []
    readonly property var artists: doc.artists || []
    readonly property var artistsCarousel: doc.artistsCarousel || []
    readonly property var playlists: doc.playlists || []
    readonly property var mp: doc.mostPopular || ({})
    readonly property bool loading: doc.loading === true
    readonly property int tab: doc.tab || 0
    readonly property int filterIndex: doc.filterIndex || 0
    // The query echo (search_qt::SearchPageDoc.query). Read only as an
    // IDENTITY: when it changes, the page on screen is a different result set
    // and any pending "fade the tail" threshold is stale (see clearFade).
    readonly property string query: doc.query || ""
    readonly property bool hasResults: albums.length + tracks.length + artists.length + playlists.length > 0
    readonly property int previewCap: 6

    // ======================= skeleton plumbing ===========================
    // ONE 900ms Timer drives EVERY placeholder in this view (QbzSkeleton's
    // preferred drive mode). GATING RULE: freeze on NOT VISIBLE — the view
    // hidden, or the window minimized/hidden. NEVER on lost focus (a tiling
    // desktop keeps windows visible and unfocused).
    property bool skelPhase: false
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true

    // A card is pending while it HAS a cover url and no local path yet.
    // Recomputed only when the document is republished (which is also when
    // covers land), never per frame.
    function anyPending(rows) {
        for (var i = 0; i < rows.length; i++) {
            if ((rows[i].artUrl || "") !== "" && (rows[i].artPath || "") === "") return true
        }
        return false
    }
    readonly property bool heroPending: {
        var k = mp.kind || ""
        if (k === "" || k === "artist") return false
        var c = k === "album" ? (mp.album || ({})) : (mp.track || ({}))
        return (c.artUrl || "") !== "" && (c.artPath || "") === ""
    }
    readonly property bool artPending: anyPending(albums) || anyPending(tracks)
        || anyPending(playlists) || heroPending
    // The pulse must OUTLIVE `artPending`: that flag drops when the last PATH
    // lands, but each of those cards still has a decode + canvas raster ahead
    // of it and its placeholder is still up (QbzSkeleton's handover). Without
    // the grace the tiles freeze mid-shimmer for the rest of the wait.
    property bool artHold: false
    Timer { id: artHoldOff; interval: 1500; onTriggered: root.artHold = false }
    onArtPendingChanged: { root.artHold = true; artHoldOff.restart() }

    // "Load more" has NO bridge-side busy flag: search_qt.rs only sets
    // doc.loading in submit(), never in load_more(). So the appended-page
    // placeholder is tracked locally — armed on the click, disarmed the
    // moment that tab's array actually grows. A page that comes back empty
    // (end of the result set) never grows it, which is why every appended
    // placeholder carries settleMs (QbzLoadMore mounts its own with the same
    // 8000ms bound this file used when it drew the block by hand).
    property int moreTab: -1
    property int moreFrom: 0
    function rowsFor(t) {
        return t === 1 ? albums : t === 2 ? tracks
            : t === 3 ? artists : t === 4 ? playlists : []
    }
    readonly property bool morePending: root.moreTab >= 0
        && root.moreTab === root.tab
        && root.rowsFor(root.moreTab).length === root.moreFrom

    // ---- the appended TAIL fades in (the "smooth" half) -------------------
    // Only the tail, and only once. Both halves of that sentence are load
    // bearing:
    //   - only the TAIL, because every republish of this document hands the
    //     views a brand-new array and recreates EVERY delegate. search_qt.rs
    //     republishes on the artwork pass, on tab_changed, on filter_changed
    //     and on apply_pin_change (its doc comment at :990 spells this out).
    //     A blanket fade would therefore re-dissolve the whole grid when a
    //     cover lands or a pin is clicked — a flicker, not a polish. So a
    //     delegate fades only when its index is at or past the row count that
    //     was on screen when the button was pressed.
    //   - only ONCE, because that threshold would otherwise survive into the
    //     NEXT republish of the same page (load_more publishes the rows, then
    //     the artwork pass publishes again ~a second later) and blink the new
    //     cards a second time. `fadeRetire` drops it a beat after the page
    //     lands.
    // `fadeTab` scopes it to the tab that asked: the four tabs share one
    // document, so a threshold armed on Albums must not fade Playlists.
    property int fadeTab: -1
    property int fadeFrom: -1
    function fadeAt(t, i) {
        return root.fadeTab === t && root.fadeFrom >= 0 && i >= root.fadeFrom
    }
    function clearFade() {
        root.fadeTab = -1
        root.fadeFrom = -1
    }
    Timer {
        id: fadeRetire
        // > the 220ms delegate fade, and long enough for the views to have
        // built the new delegates (a model swap is applied in the polish
        // step, not synchronously).
        interval: 300
        onTriggered: root.clearFade()
    }
    onMorePendingChanged: if (!root.morePending && root.fadeFrom >= 0) fadeRetire.restart()
    // A new query, a filter re-query or a tab switch is a NEW identity — the
    // stale threshold must never fade the first page of the next thing.
    onQueryChanged: root.clearFade()
    onFilterIndexChanged: root.clearFade()
    onTabChanged: root.clearFade()

    function armLoadMore(t) {
        root.moreTab = t
        root.moreFrom = root.rowsFor(t).length
        // Armed here, for all four tabs at once: the tail starts exactly
        // where the current page ends. BEFORE the bridge call, because the
        // reply may republish before this function yields.
        root.fadeTab = t
        root.fadeFrom = root.moreFrom
        moreSettle.restart()
        QbzBridge.searchLoadMore(t)
    }
    // `morePending` clears only when the tab's array GROWS — and search_qt::
    // load_more never republishes on a failed fetch (main.rs just logs), nor
    // grows the array on a page whose rows were all blacklisted away. Left
    // unbounded, a stuck `morePending` now DISARMS the button forever (the
    // old LoadMoreButton stayed clickable; QbzLoadMore's busy arm does not)
    // and keeps the placeholder block's height reserved after its shimmer
    // settles. Same 8s bound ArtistView.releaseSettle uses for the identical
    // reason, after which the button comes back armed for a retry.
    Timer {
        id: moreSettle
        interval: 8000
        repeat: false
        onTriggered: root.moreTab = -1
    }

    Timer {
        interval: 900
        repeat: true
        running: (root.loading || root.artPending || root.artHold || root.morePending)
            && root.visible && root.windowShowing
        onTriggered: root.skelPhase = !root.skelPhase
    }

    // Per-item cover placeholder, mounted by every card delegate over the
    // 200x200 artwork square. A bare Rectangle, so it does not eat the
    // card's hover/click areas underneath; it clears when THIS card's cover
    // lands, so a page resolves progressively instead of as a lump.
    // ARTISTS are deliberately excluded everywhere: ArtistCard already draws
    // a designed round gradient+glyph portrait for a missing cover, which
    // reads as a portrait rather than as a hole (same rule as LibraryView).
    // NOTE: an inline `component` does NOT see this file's outer ids, so it
    // takes NO `root.` reference — every call site passes `phase:` itself.
    component CardArtSkeleton: QbzSkeleton {
        property var card: ({})
        variant: "art"
        width: 200
        height: 200
        // HANDOVER, not "the path landed": the cards seal their RoundedImage
        // away, so this rides QbzSkeleton's probe arm — the same pixmap-cache
        // entry the card is loading, so no second decode — and retires when
        // that decode finishes. Gating on `artPath !== ""` (what this used to
        // do) drops the placeholder while the card's canvas is still blank.
        pending: (card.artUrl || "") !== ""
        coverSource: card.artPath || ""
        // A cover whose download fails republishes the document with an
        // empty artPath — without this the tile would shimmer forever.
        settleMs: 6000
    }

    // Track hero (SearchTrackHero: the 200x246 track card + the quality
    // label as TEXT under the meta).
    component SearchTrackHero: Rectangle {
        property var card: ({})
        property string qualityLabel: ""
        color: "transparent"
        readonly property bool overlayOn: heroArtArea.containsMouse

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
                    source: card.artPath || ""
                    radius: theme.radiusSm
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: parent.parent.overlayOn ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                Rectangle {
                    anchors.centerIn: parent
                    width: 44
                    height: 44
                    radius: 22
                    opacity: parent.parent.overlayOn ? 1.0 : 0.0
                    color: heroPlayArea.containsMouse ? "#d6ffffff" : "#ffffff"
                    QbzIcon { name: "play-fill"; width: 18; height: 18; anchors.centerIn: parent; tintName: "black" }
                    MouseArea {
                        id: heroPlayArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzPlayer.playTrack(card.id)
                    }
                }
                MouseArea {
                    id: heroArtArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzPlayer.playTrack(card.id)
                }
            }
            Item { width: 1; height: 6 }
            Text {
                width: 200
                height: 20
                text: card.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                width: 200
                height: 18
                text: QbzSession.tr("Track", QbzSession.trRev) + " • " + (card.artist || "")
                color: theme.textMuted
                font.pixelSize: theme.fontLink - 1
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Text {
                visible: qualityLabel !== ""
                width: 200
                text: qualityLabel
                color: theme.textMuted
                font.pixelSize: 11
                elide: Text.ElideRight
            }
        }
    }

    // Track row (primitives/TrackRow.slint, 50px — number/play, title +
    // explicit, artist, duration, quality, ⋯ menu).
    //
    // "Load more (loaded / total)" WAS an inline `LoadMoreButton` here. It is
    // now controls/QbzLoadMore.qml with `bordered: true` — the same 32-tall
    // radiusSm pill on surfaceCard / surfaceElevated-on-hover, 1px
    // borderSubtle, 13px textSecondary label, `implicitWidth + 36` hit box,
    // at y:6 inside a 44-tall box. That arm was written FROM this component,
    // so the idle pixels are unchanged; what it adds is the placeholder block
    // underneath and the busy disarm. The host still owns the whole label
    // string (it carries the "n / total" counter) and the whole visibility
    // rule — both are passed in at the four mount sites below.

    // searchType filter radio (FilterRadio: 14px ring + 8px accent dot).
    component FilterRadio: Item {
        property string label: ""
        property bool selected: false
        signal picked()
        width: frRow.width
        height: 22
        Row {
            id: frRow
            spacing: 6
            height: parent.height
            Rectangle {
                width: 14
                height: 14
                radius: 7
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                border.width: 1.5
                border.color: parent.parent.selected ? theme.accent : theme.textMuted
                Rectangle {
                    width: 8
                    height: 8
                    radius: 4
                    anchors.centerIn: parent
                    color: parent.parent.selected ? theme.accent : "transparent"
                }
            }
            Text {
                height: parent.height
                text: parent.parent.label
                color: (parent.parent.selected || frArea.containsMouse) ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
            }
        }
        MouseArea {
            id: frArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.picked()
        }
    }

    // ============================ the view ================================
    Column {
        anchors.fill: parent
        spacing: 0

        // --- Row 1: title (left) + filter radios (right) -------------------
        Item {
            width: parent.width
            height: 56
            Text {
                x: 32
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Search", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
            }
            Row {
                visible: root.hasResults
                anchors.right: parent.right
                anchors.rightMargin: 32
                anchors.verticalCenter: parent.verticalCenter
                spacing: 16
                FilterRadio { label: QbzSession.tr("Main Artist", QbzSession.trRev); selected: root.filterIndex === 1; onPicked: QbzBridge.searchFilterChanged(1) }
                FilterRadio { label: QbzSession.tr("Performer", QbzSession.trRev); selected: root.filterIndex === 2; onPicked: QbzBridge.searchFilterChanged(2) }
                FilterRadio { label: QbzSession.tr("Composer", QbzSession.trRev); selected: root.filterIndex === 3; onPicked: QbzBridge.searchFilterChanged(3) }
                FilterRadio { label: QbzSession.tr("Label", QbzSession.trRev); selected: root.filterIndex === 4; onPicked: QbzBridge.searchFilterChanged(4) }
                FilterRadio { label: QbzSession.tr("Release Name", QbzSession.trRev); selected: root.filterIndex === 5; onPicked: QbzBridge.searchFilterChanged(5) }
                Rectangle {
                    visible: root.filterIndex !== 0
                    width: 24
                    height: 24
                    radius: 12
                    anchors.verticalCenter: parent.verticalCenter
                    color: clrArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    QbzIcon { name: "x"; width: 13; height: 13; anchors.centerIn: parent; tintName: clrArea.containsMouse ? "textPrimary" : "muted" }
                    MouseArea {
                        id: clrArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.searchFilterChanged(0)
                    }
                }
            }
        }

        // --- Row 2: the five-tab strip (collapsed during the initial load) --
        Item {
            visible: !(root.loading && !root.hasResults)
            width: parent.width
            height: 40
            Row {
                x: 32
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 8
                spacing: 22
                Repeater {
                    model: [
                        { "t": 0, "label": QbzSession.tr("All", QbzSession.trRev) },
                        { "t": 1, "label": QbzSession.tr("Albums", QbzSession.trRev) },
                        { "t": 2, "label": QbzSession.tr("Tracks", QbzSession.trRev) },
                        { "t": 3, "label": QbzSession.tr("Artists", QbzSession.trRev) },
                        { "t": 4, "label": QbzSession.tr("Playlists", QbzSession.trRev) },
                    ]
                    delegate: Column {
                        required property var modelData
                        spacing: 5
                        Text {
                            text: modelData.label
                            color: root.tab === modelData.t ? theme.textPrimary
                                : (stabArea.containsMouse ? theme.textSecondary : theme.textMuted)
                            font.pixelSize: 14
                            font.weight: root.tab === modelData.t ? theme.weightBold : theme.weightMedium
                            MouseArea {
                                id: stabArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: QbzBridge.searchTabChanged(modelData.t)
                            }
                        }
                        Rectangle {
                            width: parent.width
                            height: 2
                            radius: 1
                            color: root.tab === modelData.t ? theme.accent : "transparent"
                        }
                    }
                }
            }
        }

        // --- Scrollable body ------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 56 - 40
            Flickable {
                id: bodyFlick
                anchors.fill: parent
                contentWidth: width
                contentHeight: bodyCol.height + 32
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: bodyCol
                    x: 32
                    width: bodyFlick.width - 64
                    spacing: 28

                    // Fade in once the search completes (SearchResultsView).
                    opacity: root.loading ? 0.0 : 1.0
                    Behavior on opacity { NumberAnimation { duration: 280; easing.type: Easing.InOutQuad } }

                    // ---- Most popular + Artists (All tab) ------------------
                    Row {
                        visible: root.tab === 0 && (root.mp.kind || "") !== ""
                        width: parent.width
                        spacing: 24
                        // Hero column, fixed 200px (the carousel always starts
                        // at the same x regardless of the hero kind).
                        Column {
                            width: 200
                            spacing: 12
                            Row {
                                spacing: 8
                                QbzIcon { name: "award"; width: 18; height: 18; tintName: "warning" }
                                Text {
                                    text: QbzSession.tr("Most popular", QbzSession.trRev)
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontSection
                                    font.weight: theme.weightSemibold
                                }
                            }
                            Item {
                                width: 200
                                height: 246
                                ArtistCard {
                                    visible: root.mp.kind === "artist"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    item: root.mp.artist || ({})
                                    artSource: (root.mp.artist || ({})).artPath || ""
                                    // Same two fields the album hero below
                                    // carries: the pin state (or the glyph
                                    // lies and the first click UN-pins) and
                                    // the REMOTE url the pin payload stores
                                    // (artPath is a local cache path).
                                    isPinned: (root.mp.artist || ({})).isPinned === true
                                    artworkUrl: (root.mp.artist || ({})).artUrl || ""
                                }
                                AlbumCard {
                                    visible: root.mp.kind === "album"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    albumId: (root.mp.album || ({})).id || ""
                                    title: (root.mp.album || ({})).title || ""
                                    artist: (root.mp.album || ({})).artist || ""
                                    artistId: (root.mp.album || ({})).artistId || ""
                                    genre: (root.mp.album || ({})).genre || ""
                                    year: (root.mp.album || ({})).year || ""
                                    qualityTier: (root.mp.album || ({})).qualityTier || ""
                                    artSource: (root.mp.album || ({})).artPath || ""
                                    // search_qt::map_album stamps the heart
                                    // from fav_cache; false inverted the
                                    // first click on an album already in the
                                    // library.
                                    isFavorite: (root.mp.album || ({})).isFavorite === true
                                    // The row carries the pin state
                                    // (search_qt `map_album`); hardcoding
                                    // false made the glyph lie and turned
                                    // the first click into an UN-pin.
                                    isPinned: (root.mp.album || ({})).isPinned === true
                                    artworkUrl: (root.mp.album || ({})).artUrl || ""
                                }
                                SearchTrackHero {
                                    visible: root.mp.kind === "track"
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    card: root.mp.track || ({})
                                    qualityLabel: root.mp.qualityLabel || ""
                                }
                                // Hero cover placeholder (album/track arms —
                                // the artist arm keeps ArtistCard's designed
                                // round portrait).
                                CardArtSkeleton {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    card: root.mp.kind === "album" ? (root.mp.album || ({}))
                                        : root.mp.kind === "track" ? (root.mp.track || ({}))
                                        : ({})
                                    phase: root.skelPhase
                                }
                            }
                        }
                        // Artists carousel — fills the rest of the row.
                        Column {
                            visible: root.artistsCarousel.length > 0
                            width: parent.width - 224
                            spacing: 12
                            QbzSectionHeader {
                                title: QbzSession.tr("Artists", QbzSession.trRev)
                                showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                                onViewAllClicked: QbzBridge.searchTabChanged(3)
                            }
                            ListView {
                                width: parent.width
                                height: 246
                                orientation: ListView.Horizontal
                                spacing: 32
                                clip: true
                                boundsBehavior: Flickable.StopAtBounds
                                model: root.artistsCarousel
                                delegate: ArtistCard {
                                    item: modelData
                                    artSource: modelData.artPath || ""
                                    isPinned: modelData.isPinned === true
                                    artworkUrl: modelData.artUrl || ""
                                }
                            }
                        }
                    }
                    // Artists carousel when there is no most-popular hero.
                    Column {
                        visible: root.tab === 0 && (root.mp.kind || "") === "" && root.artists.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Artists", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(3)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.artists
                            delegate: ArtistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                                isPinned: modelData.isPinned === true
                                artworkUrl: modelData.artUrl || ""
                            }
                        }
                    }

                    // ---- Albums --------------------------------------------
                    Column {
                        visible: root.tab === 0 && root.albums.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Albums", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(1)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.albums
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: 200
                                height: 246
                                AlbumCard {
                                    albumId: modelData.id
                                    title: modelData.title
                                    artist: modelData.artist
                                    artistId: modelData.artistId
                                    genre: modelData.genre
                                    year: modelData.year
                                    qualityTier: modelData.qualityTier
                                    artSource: modelData.artPath || ""
                                    // search_qt::map_album stamps the heart
                                    // from fav_cache; false made the glyph lie
                                    // and inverted the first click.
                                    isFavorite: modelData.isFavorite === true
                                    isPinned: modelData.isPinned === true
                                    artworkUrl: modelData.artUrl || ""
                                }
                                CardArtSkeleton {
                                    card: modelData
                                    phase: root.skelPhase
                                    cellIndex: index
                                }
                            }
                        }
                    }
                    // Albums tab: the full grid + Load more.
                    GridView {
                        visible: root.tab === 1
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 24) / 224))) * 270 - 24)
                        cellWidth: 224
                        cellHeight: 270
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.albums
                        delegate: Item {
                            id: albumCell
                            required property var modelData
                            required property int index
                            width: 200
                            height: 246
                            // The appended tail dissolves in; every other
                            // delegate is created at 1.0 with the Behavior
                            // DISABLED, so the republishes that recreate this
                            // grid (artwork, pins, tab switch) cost no fade at
                            // all. `opacity` is a plain value, never a
                            // binding, so the assignment below clobbers
                            // nothing.
                            readonly property bool fadeIn: root.fadeAt(1, albumCell.index)
                            opacity: 0
                            Behavior on opacity {
                                enabled: albumCell.fadeIn
                                NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                            }
                            Component.onCompleted: opacity = 1
                            AlbumCard {
                                albumId: modelData.id
                                title: modelData.title
                                artist: modelData.artist
                                artistId: modelData.artistId
                                genre: modelData.genre
                                year: modelData.year
                                qualityTier: modelData.qualityTier
                                artSource: modelData.artPath || ""
                                isFavorite: modelData.isFavorite === true
                                isPinned: modelData.isPinned === true
                                artworkUrl: modelData.artUrl || ""
                            }
                            CardArtSkeleton {
                                card: modelData
                                phase: root.skelPhase
                                cellIndex: index
                            }
                        }
                    }
                    // The page that "Load more" asked for, in the shape it
                    // will arrive in: one row of 224x270 cells on one
                    // animator, now drawn BELOW the button by QbzLoadMore
                    // (the standalone block that used to sit here was above
                    // it). It disappears the moment the array grows, or
                    // settles out after 8s if the page comes back empty.
                    //
                    // `visible` merges the two gates that were split before:
                    // the mount said `visible: root.tab === 1` and that
                    // binding OVERRODE the component's own `loaded < total`,
                    // so the pill survived past the end of the result set
                    // ("Load more (20 / 20)"). Both live in one expression
                    // now, which is what the component always meant.
                    QbzLoadMore {
                        visible: root.tab === 1
                            && root.albums.length < (root.doc.albumsTotal || 0)
                        width: parent.width
                        bordered: true
                        buttonHeight: 44
                        label: QbzSession.tr("Load more", QbzSession.trRev)
                            + " (" + root.albums.length + " / " + (root.doc.albumsTotal || 0) + ")"
                        // morePending is already scoped to the active tab
                        // (moreTab === tab), so the other three mounts cannot
                        // see a busy that is not theirs.
                        busy: root.morePending
                        skeleton: "cards"
                        cellW: 224
                        cellH: 270
                        onClicked: root.armLoadMore(1)
                    }

                    // ---- Tracks --------------------------------------------
                    Column {
                        visible: (root.tab === 0 || root.tab === 2) && root.tracks.length > 0
                        width: parent.width
                        spacing: 4
                        QbzSectionHeader {
                            title: QbzSession.tr("Tracks", QbzSession.trRev)
                            showViewAll: root.tab === 0 && (root.doc.tracksTotal || 0) > root.previewCap
                            viewAllAccent: true
                            showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(2)
                        }
                        Repeater {
                            model: root.tab === 2 ? root.tracks : root.tracks.slice(0, root.previewCap)
                            delegate: TrackRow {
                                id: trackCell
                                // Tail fade, tab 2 only: this Repeater also
                                // draws the All tab's 6-row preview, which has
                                // no Load more of its own — `fadeAt` keys on
                                // the tab that armed the threshold, so the
                                // preview can never inherit it.
                                readonly property bool fadeIn: root.fadeAt(2, index)
                                opacity: 0
                                Behavior on opacity {
                                    enabled: trackCell.fadeIn
                                    NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                                }
                                Component.onCompleted: opacity = 1
                                item: modelData
                                number: index + 1
                                menuShowFavorite: false
                                onPlayRequested: QbzPlayer.playTrack(item.id)
                                onEnqueueRequested: function (m) { QbzPlayer.enqueueTrack(item.id, m) }
                                // MyQBZ "Add to mixtape" — the HOST builds the
                                // AddItem array (TrackRow does not know
                                // itemType/source).
                                //
                                // SOURCE: the results page has ONE source.
                                // `search_qt::TrackRow` (:171) carries no
                                // source word because every row is mapped from
                                // a Qobuz `Track` (`map_track`, :320-336) and
                                // the local "on this device" sections are not
                                // ported (search_qt.rs:24-27) — the cortinilla
                                // rows, which CAN be local, carry their own
                                // `source` field and are a different document.
                                // `item.id` is a Qobuz catalog id by
                                // construction, not by assumption here.
                                onMixtapeRequested: QbzMyQbzAdd.open(JSON.stringify([{
                                    "itemType": "track", "source": "qobuz",
                                    "sourceItemId": item.id, "title": item.title || "",
                                    "subtitle": item.artist || "", "artworkUrl": "",
                                    "year": null, "trackCount": null
                                }]))
                            }
                        }
                    }
                    // Tracks: the appended page arrives as TrackRows, so the
                    // placeholder is the "rows" arm — 50px, the row height
                    // TrackRow.qml declares, on the 4px pitch the Column
                    // above uses (`spacing: 4`). The full-page loading
                    // skeleton further down passes rowGap: 0 because it
                    // covers a whole viewport, where the pitch stands next to
                    // nothing; these two rows stand directly under the last
                    // real row, so they use the real one. TWO rows — the
                    // owner's "una o dos filas".
                    QbzLoadMore {
                        visible: root.tab === 2
                            && root.tracks.length < (root.doc.tracksTotal || 0)
                        width: parent.width
                        bordered: true
                        buttonHeight: 44
                        label: QbzSession.tr("Load more", QbzSession.trRev)
                            + " (" + root.tracks.length + " / " + (root.doc.tracksTotal || 0) + ")"
                        busy: root.morePending
                        skeleton: "rows"
                        rowH: 50
                        rowGap: 4
                        rowCount: 2
                        // TrackRow draws 36px art in its 50px row, and the
                        // standalone block this replaced said so too.
                        rowArtSize: 36
                        onClicked: root.armLoadMore(2)
                    }

                    // ---- Artists grid (per-type tab) ------------------------
                    GridView {
                        visible: root.tab === 3
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 16) / 216))) * 262 - 16)
                        cellWidth: 216
                        cellHeight: 262
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.artists
                        delegate: ArtistCard {
                            id: artistCell
                            // Tail fade — same rule as the album grid. This
                            // delegate declares no REQUIRED properties, so
                            // `index` stays the injected context property the
                            // view provides (adding a required one here would
                            // switch the whole delegate to required-property
                            // mode and strand `modelData`).
                            readonly property bool fadeIn: root.fadeAt(3, index)
                            opacity: 0
                            Behavior on opacity {
                                enabled: artistCell.fadeIn
                                NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                            }
                            Component.onCompleted: opacity = 1
                            item: modelData
                            artSource: modelData.artPath || ""
                            isPinned: modelData.isPinned === true
                            artworkUrl: modelData.artUrl || ""
                        }
                    }
                    // Artists: this grid's own 216x262 pitch, NOT the 224x270
                    // the album/playlist grids use — the cells here are the
                    // ArtistCard portrait footprint: 200-wide ROUND cells on
                    // a 16px gutter (the GridView above says (width+16)/216),
                    // both of which the standalone block this replaces passed
                    // and QbzLoadMore now passes through.
                    QbzLoadMore {
                        visible: root.tab === 3
                            && root.artists.length < (root.doc.artistsTotal || 0)
                        width: parent.width
                        bordered: true
                        buttonHeight: 44
                        label: QbzSession.tr("Load more", QbzSession.trRev)
                            + " (" + root.artists.length + " / " + (root.doc.artistsTotal || 0) + ")"
                        busy: root.morePending
                        skeleton: "cards"
                        cellW: 216
                        cellH: 262
                        roundCells: true
                        cardGutter: 16
                        onClicked: root.armLoadMore(3)
                    }

                    // ---- Playlists ------------------------------------------
                    Column {
                        visible: root.tab === 0 && root.playlists.length > 0
                        width: parent.width
                        spacing: 12
                        QbzSectionHeader {
                            title: QbzSession.tr("Playlists", QbzSession.trRev)
                            showViewAll: true
                                viewAllAccent: true
                                showChevrons: false
                            onViewAllClicked: QbzBridge.searchTabChanged(4)
                        }
                        ListView {
                            width: parent.width
                            height: 246
                            orientation: ListView.Horizontal
                            spacing: 32
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: root.playlists
                            delegate: Item {
                                required property var modelData
                                required property int index
                                width: 200
                                height: 246
                                PlaylistCard {
                                    // artworkUrl is not passed: the card
                                    // defaults it to `item.artUrl`, which is
                                    // this row's remote cover.
                                    item: modelData
                                    artSource: modelData.artPath || ""
                                    isPinned: modelData.isPinned === true
                                }
                                CardArtSkeleton {
                                    card: modelData
                                    phase: root.skelPhase
                                    cellIndex: index
                                }
                            }
                        }
                    }
                    GridView {
                        visible: root.tab === 4
                        width: parent.width
                        height: Math.max(0, Math.ceil(count / Math.max(1, Math.floor((width + 24) / 224))) * 270 - 24)
                        cellWidth: 224
                        cellHeight: 270
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        interactive: false
                        model: root.playlists
                        delegate: Item {
                            id: playlistCell
                            required property var modelData
                            required property int index
                            width: 200
                            height: 246
                            // Tail fade — same rule as the album grid.
                            readonly property bool fadeIn: root.fadeAt(4, playlistCell.index)
                            opacity: 0
                            Behavior on opacity {
                                enabled: playlistCell.fadeIn
                                NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                            }
                            Component.onCompleted: opacity = 1
                            PlaylistCard {
                                item: modelData
                                artSource: modelData.artPath || ""
                                isPinned: modelData.isPinned === true
                            }
                            CardArtSkeleton {
                                card: modelData
                                phase: root.skelPhase
                                cellIndex: index
                            }
                        }
                    }
                    // Playlists share the album grid's 224x270 pitch.
                    QbzLoadMore {
                        visible: root.tab === 4
                            && root.playlists.length < (root.doc.playlistsTotal || 0)
                        width: parent.width
                        bordered: true
                        buttonHeight: 44
                        label: QbzSession.tr("Load more", QbzSession.trRev)
                            + " (" + root.playlists.length + " / " + (root.doc.playlistsTotal || 0) + ")"
                        busy: root.morePending
                        skeleton: "cards"
                        cellW: 224
                        cellH: 270
                        onClicked: root.armLoadMore(4)
                    }
                }

                // ---- Loading skeleton ---------------------------------------
                // Shape of the tab that is coming, not a spinner. Mounted for
                // the WHOLE of `loading`, not only the first search: bodyCol
                // fades to 0 while a new query is in flight, so a re-search
                // used to show an empty page with no affordance at all.
                // COST: at most 8 animators on the All tab (3 title bars, 1
                // hero card, 2 card composites, 1 slim-row composite); one
                // composite covers a whole viewport row on ONE animator.
                Item {
                    id: searchSkel
                    visible: root.loading
                    x: 32
                    y: 0
                    width: bodyFlick.width - 64
                    height: bodyFlick.height

                    // All tab: hero + artists carousel + albums + tracks.
                    Column {
                        visible: root.tab === 0
                        width: parent.width
                        spacing: 28
                        Row {
                            width: parent.width
                            spacing: 24
                            Column {
                                width: 200
                                spacing: 12
                                QbzSkeleton { variant: "block"; width: 160; height: 22; phase: root.skelPhase }
                                QbzSkeleton { variant: "card"; width: 200; phase: root.skelPhase }
                            }
                            Column {
                                width: parent.width - 224
                                spacing: 12
                                QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                                QbzSkeleton {
                                    variant: "cardGrid"
                                    width: parent.width
                                    height: 246
                                    cellW: 232
                                    cellH: 246
                                    roundCells: true
                                    phase: root.skelPhase
                                }
                            }
                        }
                        Column {
                            width: parent.width
                            spacing: 12
                            QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                            QbzSkeleton {
                                variant: "cardGrid"
                                width: parent.width
                                height: 246
                                cellW: 232
                                cellH: 246
                                phase: root.skelPhase
                            }
                        }
                        Column {
                            width: parent.width
                            spacing: 8
                            QbzSkeleton { variant: "block"; width: 180; height: 22; phase: root.skelPhase }
                            QbzSkeleton {
                                variant: "rowList"
                                width: parent.width
                                height: 300
                                rowH: 50
                                rowGap: 0
                                rowArtSize: 36
                                phase: root.skelPhase
                            }
                        }
                    }

                    // Albums / Playlists tabs: the 224x270 grid.
                    QbzSkeleton {
                        visible: root.tab === 1 || root.tab === 4
                        variant: "cardGrid"
                        anchors.fill: parent
                        cellW: 224
                        cellH: 270
                        phase: root.skelPhase
                    }
                    // Tracks tab: the 50px TrackRow list.
                    QbzSkeleton {
                        visible: root.tab === 2
                        variant: "rowList"
                        anchors.fill: parent
                        rowH: 50
                        rowGap: 0
                        rowArtSize: 36
                        phase: root.skelPhase
                    }
                    // Artists tab: round cells on the 216x262 pitch.
                    QbzSkeleton {
                        visible: root.tab === 3
                        variant: "cardGrid"
                        anchors.fill: parent
                        cellW: 216
                        cellH: 262
                        roundCells: true
                        phase: root.skelPhase
                    }
                }
                Text {
                    visible: !root.loading && !root.hasResults
                    x: 32
                    y: 8
                    text: QbzSession.tr("No results.", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: 14
                }
            }
            QbzScrollBar {
                target: bodyFlick
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
            }
        }
    }
}
