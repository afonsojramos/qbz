// PurchaseAlbumsCollection — one collection of purchased albums, drawn as a
// grid or as a list. 1:1 port of purchases/PurchasesView.slint's
// `AlbumsCollection` (:1155-1210).
//
// "One collection" = either the whole flat set, or ONE group's albums when the
// toolbar's grouping is on; the host mounts as many of these as there are
// groups, which is what keeps the group headers and their bodies in one place.
//
// GEOMETRY, off the .slint: grid cards 162x232 on a 16px gap (the Tauri
// `auto-fill minmax(162px, 1fr)` column math, reproduced exactly), list rows
// 60px on a 4px gap. The root is content-sized in both arms so the page's one
// Flickable owns the scroll — there is no nested scroller here.
//
// NO WINDOWING. views/AlbumCollection.qml samples a visible band every 80ms and
// mounts only the cards inside it; that machinery is coupled to ITS delegate
// (cards/AlbumCard.qml) and its host contract, and §15.1 forbids building
// virtualisation of our own. Every purchased album therefore mounts, exactly as
// it does in Tauri and in the .slint. Recorded as a known cost, not an
// oversight: a very large purchase library will pay for it on entry.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Item {
    id: root

    /// Rows of `list_json.albums` (§G.2) — already searched / sorted / filtered
    /// by the controller.
    property var albums: []
    /// "grid" | "list".
    property string viewMode: "grid"
    signal openAlbum(string albumId)

    QbzTheme { id: theme }

    readonly property int cardWidth: 162
    readonly property int cardHeight: 232
    readonly property int cardGap: 16
    readonly property int listRowHeight: 60
    readonly property int listGap: 4
    /// The column-label strip above the rows (PurchaseListHeader), list arm only.
    readonly property int listHeaderHeight: 28

    readonly property int columns: Math.max(
        1, Math.floor((width + root.cardGap) / (root.cardWidth + root.cardGap)))
    readonly property int gridRows:
        Math.ceil(root.albums.length / Math.max(1, root.columns))

    width: parent ? parent.width : 0
    // Content-sized: the page Flickable reads this through its Column.
    height: root.viewMode === "list"
        ? (root.albums.length > 0
            ? root.listHeaderHeight + root.listGap
              + root.albums.length * root.listRowHeight
              + (root.albums.length - 1) * root.listGap
            : 0)
        : (root.gridRows > 0
            ? root.gridRows * root.cardHeight + (root.gridRows - 1) * root.cardGap
            : 0)

    // --- Grid -------------------------------------------------------------
    Item {
        visible: root.viewMode !== "list"
        anchors.fill: parent
        Repeater {
            // The model is gated on the arm so the hidden arm builds no
            // delegates at all (the AlbumCollection convention) — a hidden
            // Repeater still instantiates everything otherwise.
            model: root.viewMode !== "list" ? root.albums : []
            delegate: PurchaseGridCard {
                required property var modelData
                required property int index
                x: (index % root.columns) * (root.cardWidth + root.cardGap)
                y: Math.floor(index / root.columns) * (root.cardHeight + root.cardGap)
                width: root.cardWidth
                height: root.cardHeight
                album: modelData
                onClicked: root.openAlbum(modelData.id || "")
            }
        }
    }

    // --- List -------------------------------------------------------------
    Column {
        visible: root.viewMode === "list"
        anchors.fill: parent
        spacing: root.listGap
        PurchaseListHeader {
            visible: root.viewMode === "list" && root.albums.length > 0
            width: parent.width
            height: root.listHeaderHeight
        }
        Repeater {
            model: root.viewMode === "list" ? root.albums : []
            delegate: PurchaseListRow {
                required property var modelData
                width: parent ? parent.width : 0
                album: modelData
                onClicked: root.openAlbum(modelData.id || "")
            }
        }
    }
}
