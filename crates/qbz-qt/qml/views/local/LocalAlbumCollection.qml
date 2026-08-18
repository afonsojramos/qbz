// The Local Library album collection — the Qt equivalent of
// discover/AlbumCollectionView.slint as LocalLibraryView.slint mounts it
// (:1255 Albums, :1601 Folders-flat, :2409 Artists detail): grid or list,
// grouped or flat, with the source badge and multi-select.
//
// WHY A CHUNK MODEL AND NOT A GridView + Repeater
// The Slint calls this a "chunked grid" and it is the one surface with the
// documented 16k-row freeze. Here EVERY arm is a single windowing ListView
// over a flat "entry" array:
//     { t: 0, label }                 -> group header (34px)
//     { t: 1, items: [...], base: n } -> one visual row of cards, or one
//                                        list row; `base` is the row's
//                                        first index in the flat album array
// so grouping costs one O(n) rebuild when the inputs change (never per
// frame) and the mounted delegate count stays proportional to the viewport,
// not to the library. Artwork is reported from the mounted entry range only,
// via the host's windowing helpers.

import QtQuick
import com.blitzfc.qbz
import "../../cards"
import "../../controls"
import "../../theme"

Item {
    id: root

    /// The LocalLibraryView root — artMap + queueWindowReport live there.
    property var view: null
    /// Stable id for this cover surface in the host's window registry. Three
    /// instances of this component can be alive at once (Albums, Folders-flat,
    /// the Artists detail grid), and they must not share a slot.
    property string surface: "collection"
    /// Flat album rows (already searched/sorted/filtered by the host).
    property var rows: []
    /// [{ letter, items: [...] }] — only read when `grouped`.
    property var groups: []
    property bool grouped: false
    property string viewMode: "grid"      // "grid" | "list"
    property bool showSource: true
    property bool selectMode: false
    /// { albumId: true } — the host owns the selection set.
    property var selected: ({})

    signal openRequested(string id)
    signal playRequested(string id)
    signal enqueueRequested(string id, string mode)
    signal toggleSelect(string id)

    QbzTheme { id: theme }

    readonly property int cellW: 220
    readonly property int cellH: 266
    readonly property int headerH: 34
    readonly property int listRowH: 56
    readonly property int cols: viewMode === "grid"
        ? Math.max(1, Math.floor(width / cellW)) : 1

    // ---------------------------------------------------------------------
    // Chunk model
    // ---------------------------------------------------------------------
    // `flat` is the album array in display order (the window report indexes
    // into it); `entries` is what the ListView actually mounts.
    property var flat: []
    property var entries: []
    /// [{ letter, index }] with `index` = the ENTRY index of that letter's
    /// header — what AlphaStrip needs to scroll here.
    property var alphaJumps: []

    function rebuild() {
        var out = []
        var flatOut = []
        var jumps = []
        var per = cols
        var i, j
        function pushChunk(items) {
            for (j = 0; j < items.length; j += per) {
                out.push({ "t": 1, "base": flatOut.length + j,
                           "items": items.slice(j, j + per) })
            }
            for (j = 0; j < items.length; j++) flatOut.push(items[j])
        }
        if (grouped) {
            for (i = 0; i < groups.length; i++) {
                var g = groups[i]
                if (!g.items || g.items.length === 0) continue
                jumps.push({ "letter": g.letter, "index": out.length })
                // The header carries the flat index its group STARTS at.
                // Without it the window report below fell back to 0 whenever
                // the first visible entry was a header — so scrolling in
                // grouped mode asked for artwork from album 0 through the
                // current letter, hundreds of covers at a time, and then
                // evicted them again. (Found by an independent review; the
                // fallback reads as harmless until you notice what `0` means
                // to `queueWindowReport`.)
                out.push({ "t": 0, "label": g.letter, "base": flatOut.length })
                pushChunk(g.items)
            }
        } else {
            pushChunk(rows)
        }
        flat = flatOut
        entries = out
        alphaJumps = jumps
        report()
    }
    onRowsChanged: rebuild()
    onGroupsChanged: rebuild()
    onGroupedChanged: rebuild()
    onViewModeChanged: rebuild()
    onColsChanged: rebuild()
    Component.onCompleted: { rebuild(); reportSoon() }
    Component.onDestruction: if (view) view.releaseWindow(root.surface)

    // ---------------------------------------------------------------------
    // Window report — the MOUNTED entry band, mapped back to flat indices.
    // ---------------------------------------------------------------------
    // TRIGGERS. A report used to leave here on `contentY` and on the model
    // swap alone, and BOTH of those fire while this surface is still hidden:
    // the tab body is mounted behind `localAlbumsLoading` and the rows land
    // in the same pass, so the one report that mattered was thrown away by
    // the `!visible` guard below and nothing was requested until the user
    // moved the list by a pixel. Every state change that can alter the
    // mounted band now reports: mount, becoming visible, the model rebuild,
    // a viewport resize, and the scroll.
    function report() {
        if (!view || !list) return
        if (!list.visible || entries.length === 0 || width <= 0) {
            view.releaseWindow(root.surface)
            return
        }
        var first = list.indexAt(4, list.contentY + 1)
        var last = list.indexAt(4, list.contentY + Math.max(1, list.height) - 1)
        if (first < 0) first = 0
        if (last < 0) last = Math.min(entries.length - 1, first + 8)
        var lo = entries[first]
        var hi = entries[last]
        // Both entry kinds now carry `base`, so a header at either edge of
        // the viewport reports ITS group's position instead of collapsing to
        // the top of the library.
        var loIdx = lo ? (lo.base || 0) : 0
        var hiIdx = hi
            ? (hi.t === 1 ? hi.base + hi.items.length - 1 : Math.max(loIdx, hi.base || 0))
            : loIdx
        view.queueWindowReport(flat, loIdx, hiIdx, root.surface)
    }

    /// Report NOW, then again once layout has settled. `indexAt` answers -1
    /// until the ListView has laid its delegates out, which is exactly the
    /// state a just-mounted / just-shown list is in — the immediate call gets
    /// the covers moving off the conservative fallback band, the second one
    /// corrects it to the real viewport.
    Timer {
        id: reportSettle
        interval: 50
        repeat: false
        onTriggered: root.report()
    }
    function reportSoon() {
        report()
        reportSettle.restart()
    }

    onVisibleChanged: {
        if (visible) reportSoon()
        else if (view) view.releaseWindow(root.surface)
    }
    onHeightChanged: report()
    Connections {
        target: root.view
        function onArtworkRefresh() { root.reportSoon() }
    }

    function jumpToEntry(entryIndex) {
        list.positionViewAtIndex(entryIndex, ListView.Beginning)
    }

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        cacheBuffer: root.viewMode === "grid" ? root.cellH * 2 : root.listRowH * 10
        boundsBehavior: Flickable.StopAtBounds
        // Recycle rows instead of destroying and rebuilding them on every
        // scroll step. Safe only NOW: the project's own note (PmListRow /
        // PmFolderMenu) is that `reuseItems` plus a per-row menu gives every
        // pooled delegate its own Popup — so the menus above had to become
        // lazy FIRST. In the other order this would install that bug.
        reuseItems: true
        model: root.entries
        onContentYChanged: root.report()
        onModelChanged: root.report()
        onHeightChanged: root.report()
        onVisibleChanged: if (visible) root.reportSoon()

        delegate: Loader {
            required property var modelData
            width: list.width
            height: modelData.t === 0 ? root.headerH
                 : root.viewMode === "grid" ? root.cellH : root.listRowH
            sourceComponent: modelData.t === 0 ? headerComp
                : root.viewMode === "grid" ? cardRowComp : listRowComp

            Component {
                id: headerComp
                Item {
                    Text {
                        x: 2
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.label
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                        font.weight: theme.weightSemibold
                    }
                }
            }

            // One visual row of album cards.
            Component {
                id: cardRowComp
                Row {
                    id: cardRow
                    /// The row entry from the ListView delegate (the Loader's
                    /// `modelData`), captured here because the Repeater below
                    /// shadows that name inside its own delegates.
                    readonly property var rowEntry: modelData
                    spacing: root.cellW - 200
                    // FIXED CELL COUNT — `root.cols`, a NUMBER, not the row's
                    // item array.
                    //
                    // This is what makes the ListView's `reuseItems` actually
                    // pay. Pooling recycles the row SHELL, but the old
                    // `model: modelData.items` handed this Repeater a new
                    // array on every recycle, so it destroyed and rebuilt its
                    // AlbumCards anyway — and the card is where the cost is:
                    // ~29 items, a RoundedImage with TWO render targets (its
                    // own layer plus the mask), image probes, a skeleton.
                    // The shell was reused and the expensive part was not.
                    //
                    // With a constant count the cells are created once and
                    // survive recycling; a scrolled or re-filtered row only
                    // reassigns `item`, which is a binding update instead of a
                    // construction. Same shape as the reference, where cheap
                    // slots stay alive and only the data moves.
                    Repeater {
                        model: root.cols
                        delegate: Item {
                            id: cardCell
                            required property int index
                            /// The album for this slot, or `null` on the last
                            /// row when it is not full. A null slot renders
                            /// nothing and keeps its width, so the row's
                            /// spacing does not collapse.
                            readonly property var modelData:
                                (cardRow.rowEntry && cardRow.rowEntry.items)
                                    ? (cardRow.rowEntry.items[cardCell.index] || null)
                                    : null
                            visible: cardCell.modelData !== null
                            width: 200
                            height: 246

                            // Right-click menu for the card (AlbumCard.slint's
                            // album-menu, local arm: no favourite, no Block —
                            // the Slint hides Block for source local/plex).
                            // The card's own ⋯ overlay button opens AlbumCard's
                            // built-in menu; see the GLUE note about aligning
                            // that one's entries with these.
                            // LAZY, like AlbumCard's own. This cell carried a
                            // SECOND popup on top of the card's, so a grid of
                            // 1267 local albums paid two Popup constructions
                            // per cell — and the ListView recycles a screenful
                            // of cells on every scroll step, which is what the
                            // owner reports as heavy scrolling here.
                            Loader {
                                id: cardMenuLoader
                                active: false
                                sourceComponent: CardMenu {
                                    menuWidth: 196
                                    entries: [
                                        { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
                                        { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                                        { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                                        { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                                        { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                                    ]
                                    onPicked: function (a) {
                                        if (a === "open") root.openRequested(cardCell.modelData.id)
                                        else if (a === "play") root.playRequested(cardCell.modelData.id)
                                        else root.enqueueRequested(cardCell.modelData.id, a)
                                    }
                                }
                            }
                            /// Build the popup on first use, then open it.
                            function openCardMenu(anchor, x, y) {
                                cardMenuLoader.active = true
                                cardMenuLoader.item.openAtCursor(anchor, x, y)
                            }

                            AlbumCard {
                                localMode: true
                                albumId: cardCell.modelData.id
                                title: cardCell.modelData.title
                                artist: cardCell.modelData.artist
                                artistId: ""
                                genre: ""
                                year: cardCell.modelData.year
                                qualityTier: cardCell.modelData.qualityTier
                                artSource: root.view
                                    ? (root.view.artMap[cardCell.modelData.artKey] || "") : ""
                                // LocalLibraryView.slint:1267 `show-source-badge`
                                // — the card takes the raw source word and the
                                // BADGE is what the flag switches (blanking
                                // `source` would also silence the card's
                                // source-aware menu gates).
                                //
                                // `sourceRaw` FIRST (contract §D.1). It carries
                                // `qobuz_purchase` and nothing else; `source`
                                // folds that into "offline" so the source chips
                                // keep filtering it, which means the card's gold
                                // purchase chip (AlbumCard.qml:588) only ever
                                // fires off the raw word. All FOUR badge sites
                                // do this — grid and list must not disagree
                                // about the same album.
                                source: cardCell.modelData.sourceRaw
                                    || cardCell.modelData.source
                                showSourceBadge: root.showSource
                                // SELECT MODE IS THE CARD'S (AlbumCard's
                                // `selectMode`/`selected`/`selectToggled`, the
                                // port of discover/AlbumCard.slint:83, :179-197,
                                // :207, :239, :465). This host used to paint its
                                // OWN 22px tick on top of the card at top-left —
                                // a THIRD geometry, and one that could only add
                                // the indicator: the card's hover PLAY button
                                // stayed live underneath it, so hovering a card
                                // you meant to tick and hitting play started the
                                // album. The card hides the whole hover action
                                // row and the pin badge in select mode and draws
                                // the reference's 24px tick in the reference's
                                // corner (top-right); all three move together,
                                // which is exactly what a host overlay cannot
                                // do. Do not re-add one.
                                selectMode: root.selectMode
                                selected: root.selected[cardCell.modelData.id] === true
                                onSelectToggled: root.toggleSelect(cardCell.modelData.id)
                                // Non-select mode only — the card routes a
                                // select-mode click to `selectToggled` and never
                                // emits this while ticking.
                                onOpenRequested: root.openRequested(cardCell.modelData.id)
                                onPlayRequested: root.playRequested(cardCell.modelData.id)
                                onEnqueueRequested: function (m) {
                                    root.enqueueRequested(cardCell.modelData.id, m)
                                }
                            }
                            // Per-item cover placeholder, handed over to the
                            // art itself: AlbumCard seals its RoundedImage
                            // away, so this uses QbzSkeleton's probe arm —
                            // `coverSource` loads the SAME pixmap-cache entry
                            // the card is loading and retires the placeholder
                            // when that decode completes, not when the path
                            // appears. A bare Rectangle, so it does not take
                            // pointer events and the card's own areas keep
                            // working underneath.
                            // settleMs is mandatory here: local artwork
                            // resolution drops keys with no cover, so an
                            // artless album would otherwise shimmer forever
                            // (see LocalLibraryView.artSettleMs).
                            QbzSkeleton {
                                variant: "art"
                                width: 200
                                height: 200
                                pending: root.view
                                    ? root.view.artWanted(cardCell.modelData.artKey) : false
                                coverSource: root.view
                                    ? root.view.artPathOf(cardCell.modelData.artKey) : ""
                                phase: root.view ? root.view.skelPhase : false
                                settleMs: root.view ? root.view.artSettleMs : 0
                                settleHold: root.view ? root.view.artPulse : false
                            }
                            // RIGHT-only, declared after the card so every
                            // left click still reaches the card's own areas.
                            MouseArea {
                                id: cardRc
                                anchors.fill: parent
                                acceptedButtons: Qt.RightButton
                                onClicked: function (mouse) {
                                    cardCell.openCardMenu(cardRc, mouse.x, mouse.y)
                                }
                            }
                            // (The multi-select tick that used to sit here is
                            //  gone — it is AlbumCard's `selectMode` now; see
                            //  the note on the card above.)
                        }
                    }
                }
            }

            // One list row.
            Component {
                id: listRowComp
                LocalAlbumRow {
                    width: list.width
                    view: root.view
                    item: modelData.items[0]
                    artSource: root.view
                        ? (root.view.artMap[modelData.items[0].artKey] || "") : ""
                    showSource: root.showSource
                    selectMode: root.selectMode
                    checked: root.selected[modelData.items[0].id] === true
                    onOpened: root.openRequested(modelData.items[0].id)
                    onPlayRequested: root.playRequested(modelData.items[0].id)
                    onEnqueueRequested: function (m) {
                        root.enqueueRequested(modelData.items[0].id, m)
                    }
                    onToggleSelect: root.toggleSelect(modelData.items[0].id)
                }
            }
        }
    }

    QbzScrollBar {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: list
        visible: list.contentHeight > list.height
    }
}
