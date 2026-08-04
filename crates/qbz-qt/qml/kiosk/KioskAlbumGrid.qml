// KioskAlbumGrid — lightweight wrapping grid of KioskCards for the kiosk
// Search/Library album tabs (shell/KioskAlbumGrid.slint). Absolute-positioned
// cells inside a vertical Flickable; card width derives from the grid width so
// it reflows.
//
// WINDOWED (in-window pattern, mirrors the desktop Carousel): the Repeater
// creates only cheap empty positioned slots for every album, but the heavy
// KioskCard (image + text) mounts ONLY for slots whose band intersects the
// viewport. A plain full grid mounted every card at once and spiked CPU on
// entry — a Pi can't afford that. So this is a Repeater of SLOTS with a gated
// Loader, never a Repeater of cards and never a GridView (whose scroll
// semantics the focus writes below could not drive).
//
// Slint's `viewport-y` is negative as you scroll down; Qt's `contentY` is its
// positive twin — every occurrence is already translated.
//
// The HOSTING VIEW publishes the grid geometry (tabs / columns / count) into
// QbzKioskNav; the grid only mirrors the focus index (ring + scroll-into-view)
// and answers the Enter pulse. The content index space skips `tabs` leading
// tab entries.

import QtQuick
import com.blitzfc.qbz

Flickable {
    id: root

    property var albums: []
    property int columns: 6
    property real gap: 16
    property real pad: 16
    signal open(string id)

    readonly property real cardW: (root.width - 2 * root.pad - (root.columns - 1) * root.gap) / root.columns
    readonly property real cardH: root.cardW + 46
    readonly property int rows: Math.ceil(root.albums.length / root.columns)

    contentWidth: root.width
    contentHeight: root.rows * (root.cardH + root.gap) + root.pad
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
    readonly property bool itemFocused: QbzKioskNav.navActive
        && QbzKioskNav.zone === "content"
        && root.focusedItem >= 0
        && root.focusedItem < root.albums.length
    readonly property real focusTop: root.pad
        + Math.floor(root.focusedItem / root.columns) * (root.cardH + root.gap)

    // Scroll the focused card's row into view. contentY is WRITTEN from this
    // function on a settled layout (Qt.callLater defers it past the current
    // binding evaluation), never read by a binding that feeds layout — the
    // AlbumCollectionView "Recursion detected" panic class.
    function scrollFocusIntoView() {
        if (!root.itemFocused)
            return
        if (root.focusTop < root.contentY)
            root.contentY = root.focusTop - 8
        else if (root.focusTop + root.cardH > root.contentY + root.height)
            root.contentY = root.focusTop + root.cardH - root.height + 8
    }

    Connections {
        target: QbzKioskNav

        function onIndexChanged() {
            Qt.callLater(root.scrollFocusIntoView)
        }

        // Enter pulse → open the focused album.
        function onActivateSeqChanged() {
            if (root.itemFocused)
                root.open(root.albums[root.focusedItem].id)
        }
    }

    Item {
        width: root.contentWidth
        height: root.contentHeight

        Repeater {
            model: root.albums

            delegate: Item {
                id: slot

                required property var modelData
                required property int index

                x: root.pad + (slot.index % root.columns) * (root.cardW + root.gap)
                y: root.pad + Math.floor(slot.index / root.columns) * (root.cardH + root.gap)
                width: root.cardW
                height: root.cardH

                // Visible band (content coords) is [contentY, contentY+height];
                // 240px margin keeps a card mounted just off-screen for smooth
                // scrolling.
                readonly property bool inWindow: (slot.y + slot.height >= root.contentY - 240)
                    && (slot.y <= root.contentY + root.height + 240)

                Component {
                    id: cardComponent

                    KioskCard {
                        artSize: root.cardW
                        album: slot.modelData
                        navFocused: root.itemFocused && root.focusedItem === slot.index
                        onClicked: function (id) {
                            root.open(id)
                        }
                    }
                }

                Loader {
                    active: slot.inWindow
                    sourceComponent: cardComponent
                }
            }
        }
    }
}
