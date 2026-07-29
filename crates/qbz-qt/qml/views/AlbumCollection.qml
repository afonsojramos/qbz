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
        onTriggered: root.sampleBand()
    }
    Component.onCompleted: root.sampleBand()

    // One group-by section's card grid (FULL mounts — see the header note).
    // Declared before its use for readability; QML registers inline
    // components document-wide either way.
    component SectionGrid: Item {
        id: sg
        property var items: []
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
                    artSource: gcell.modelData.artPath || ""
                    isPinned: gcell.modelData.isPinned === true
                    isFavorite: false
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
                        }
                    }
                }
                // Grid arm.
                SectionGrid {
                    visible: root.viewMode !== "list"
                    items: sectionCol.modelData.albums || []
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
                rowIndex: index
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
                        artSource: cell.modelData.artPath || ""
                        isPinned: cell.modelData.isPinned === true
                        plays: root.showPlays ? (cell.modelData.plays || 0) : 0
                        isFavorite: false
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
