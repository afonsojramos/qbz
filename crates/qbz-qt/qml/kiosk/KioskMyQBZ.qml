// KioskMyQBZ — lightweight kiosk My QBZ view (shell/KioskMyQBZ.slint). Reads
// the same two grid documents the desktop MyQbzGridView reads. Tabs Mixtapes /
// Collections, simple cover cards, tap opens the detail. Mounted for BOTH the
// "mixtapes" and "collections" routes (ContentRouter.qml:98).
//
// THE TAB SWITCH IS A ROUTE, NOT A LOCAL ENUM. There is no local tab property:
// the active tab is derived from the current route, and both the tap path and
// the Enter path NAVIGATE (KioskMyQBZ.slint:106-107, :130-141, :158-168). The
// route discriminator arrives as `kind`, which ContentRouter.qml:142 computes
// as exactly `QbzShell.currentView === "collections"`, so reading it here is
// the same test the Slint's `NavState.view == ContentView.collections` is —
// and it is the property the router already assigns, in kiosk too
// (ContentRouter.qml:135-137).
//
// Because ONE live instance serves both routes (unlike Slint, where the two
// `if` mounts re-run `init => publish-nav()`), publishNav() also runs on the
// route flip — the compensation the mount-model difference requires.
//
// Nav geometry (KioskMyQBZ.slint:113-128): 2 tabs followed by the ACTIVE list
// as a 6-column grid. The clamp lives in Rust (kiosk_nav_qt.rs:271-273).
//
// Slint's `viewport-y` is negative as you scroll down; Qt's `contentY` is its
// positive twin — every occurrence below is already translated.
//
// Covers are push-only and the fallback chain has exactly three arms
// (KioskMyQBZ.slint:69-80): custom cover, else the FIRST collage cell, else
// the bare elevated square. No mosaic, no kind glyph, no placeholder — and no
// empty state, no skeleton, no spinner anywhere in this view, because the
// reference has none.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    /// "mixtape" | "collection" — assigned by ContentRouter's Binding
    /// (ContentRouter.qml:139-147), which sources it from the route.
    property string kind: "mixtape"
    readonly property bool collectionsTab: root.kind === "collection"

    readonly property real pad: 16

    // The kiosk content panel paints and rounds its own surface
    // (KioskShell.qml:409-416), exactly as KioskShell.slint's does, so the
    // view root is transparent — as the Slint root Rectangle is.
    color: "transparent"

    QbzTheme { id: theme }

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // ONE guarded parse per document. A raw JSON.parse inside a binding throws
    // on the pre-publish frame and takes the whole view down
    // (MyQbzGridView.qml:91-93).
    readonly property var doc: root.docFor(root.collectionsTab)
    function docFor(collections) {
        try {
            return JSON.parse(collections ? QbzMyQbz.collectionsJson
                                          : QbzMyQbz.mixtapesJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var cards: root.doc.cards || []

    // ---- Nav geometry publish: 2 tabs + the active list, 6 columns.
    //
    // Re-derives the card count from `collectionsTab` AT CALL TIME rather than
    // reading `cards`, and that is load-bearing. `onOnCollectionsChanged`
    // below is connected to the very notify that dirties `doc` — and through
    // it `cards` — and the handler runs BEFORE the dependent bindings
    // re-evaluate, so a read of `root.cards` from inside it counts the
    // PREVIOUS route's document. `publish()` CLAMPS `index` into the count it
    // is handed (src/kiosk_nav_qt.rs:266-274), so the wrong count parks the
    // focus ring on the wrong row. Same hazard, same remedy, as
    // KioskArtist.qml:102-110 and the note at Main.qml:233-247.
    function publishNav() {
        var d = root.docFor(root.collectionsTab)
        QbzKioskNav.publishNav(2, 6, 2 + (d.cards || []).length, false)
    }

    Component.onCompleted: root.publishNav()

    // Slint probes `collections.length + mixtapes.length` because both its
    // models are globals; here only the ACTIVE document is parsed, so the
    // probe is its length plus the route flip below — together they cover
    // every input to `count`.
    readonly property int navLenProbe: root.cards.length
    onNavLenProbeChanged: root.publishNav()
    onOnCollectionsChanged: root.publishNav()

    Connections {
        target: QbzKioskNav

        function onIndexChanged() {
            Qt.callLater(grid.scrollFocusIntoView)
        }

        // The Enter pulse. The two arms are disjoint on `index < tabs`,
        // exactly as the Slint's two watchers are (:131-141 and :207-216).
        function onActivateSeqChanged() {
            if (!QbzKioskNav.navActive || QbzKioskNav.zone !== "content")
                return
            if (QbzKioskNav.index < QbzKioskNav.tabs) {
                // Enter on a tab → the same route the tab taps navigate to.
                if (QbzKioskNav.index === 0)
                    QbzShell.navigateTo("mixtapes")
                else if (QbzKioskNav.index === 1)
                    QbzShell.navigateTo("collections")
                return
            }
            if (grid.itemFocused)
                QbzMyQbz.openCard(root.cards[grid.focusedItem].id)
        }
    }

    // =====================================================================
    // Tab strip (KioskMyQBZ.slint:146-176)
    // =====================================================================
    Rectangle {
        id: tabStrip
        anchors.left: root.left
        anchors.right: root.right
        anchors.top: root.top
        height: 48
        color: theme.surfaceMain

        Row {
            id: tabRow
            x: root.pad
            height: tabStrip.height
            spacing: 22

            MyQbzTab {
                label: root.t("Mixtapes")
                active: !root.collectionsTab
                navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === 0
                onPicked: QbzShell.navigateTo("mixtapes")
            }

            MyQbzTab {
                label: root.t("Collections")
                active: root.collectionsTab
                navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === 1
                onPicked: QbzShell.navigateTo("collections")
            }
        }

        Rectangle {
            id: hairline
            x: 0
            y: tabStrip.height - 1
            width: tabStrip.width
            height: 1
            color: theme.borderSubtle
        }
    }

    // =====================================================================
    // The grid (KioskMyQBZ.slint:178-231)
    // =====================================================================
    Flickable {
        id: grid

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: tabStrip.bottom
        anchors.bottom: root.bottom
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        readonly property int cols: 6
        readonly property real gap: 16
        readonly property real cell: (grid.width - 2 * root.pad - (grid.cols - 1) * grid.gap) / grid.cols
        // The 44px band under the cover is the name + meta lines.
        readonly property real itemH: grid.cell + 44

        contentWidth: grid.width
        contentHeight: Math.ceil(root.cards.length / grid.cols) * (grid.itemH + grid.gap) + root.pad

        readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
        readonly property bool itemFocused: QbzKioskNav.navActive
            && QbzKioskNav.zone === "content"
            && grid.focusedItem >= 0
            && grid.focusedItem < root.cards.length
        readonly property real focusTop: root.pad
            + Math.floor(grid.focusedItem / grid.cols) * (grid.itemH + grid.gap)

        // contentY is WRITTEN from this function on a settled layout
        // (Qt.callLater defers it past the current binding evaluation), never
        // read by a binding that feeds layout — the AlbumCollectionView
        // "Recursion detected" panic class.
        function scrollFocusIntoView() {
            if (!grid.itemFocused)
                return
            if (grid.focusTop < grid.contentY)
                grid.contentY = grid.focusTop - 8
            else if (grid.focusTop + grid.itemH > grid.contentY + grid.height)
                grid.contentY = grid.focusTop + grid.itemH - grid.height + 8
        }

        Item {
            id: canvas
            width: grid.contentWidth
            height: grid.contentHeight

            Repeater {
                model: root.cards

                delegate: MyQbzCardLite {
                    id: cardSlot

                    required property var modelData
                    required property int index

                    x: root.pad + (cardSlot.index % grid.cols) * (grid.cell + grid.gap)
                    y: root.pad + Math.floor(cardSlot.index / grid.cols) * (grid.itemH + grid.gap)
                    size: grid.cell
                    item: cardSlot.modelData
                    navFocused: grid.itemFocused && grid.focusedItem === cardSlot.index
                    onClicked: function (id) {
                        QbzMyQbz.openCard(id)
                    }
                }
            }
        }
    }

    // =====================================================================
    // Inline components (KioskMyQBZ.slint's two private components)
    // =====================================================================
    // An inline component does NOT share the enclosing document's scope, so
    // each one declares its own QbzTheme and takes every string as a property
    // — the same reason ViewModeMenu.qml:27 declares its own copy.

    /// MyQbzTab (KioskMyQBZ.slint:14-47) — geometrically identical to the
    /// Library / Local Library tabs: 2px ring, 2px side padding, 15px/600
    /// label, 2px underline.
    component MyQbzTab: Rectangle {
        id: tab

        property string label: ""
        property bool active: false
        property bool navFocused: false
        signal picked()

        QbzTheme { id: tabTheme }

        width: tabLabel.width + 4
        height: 48
        color: "transparent"
        border.width: tab.navFocused ? 2 : 0
        border.color: tabTheme.accent
        radius: tabTheme.radiusSm

        Text {
            id: tabLabel
            x: 2
            anchors.top: tab.top
            anchors.bottom: tabUnderline.top
            verticalAlignment: Text.AlignVCenter
            text: tab.label
            color: tab.active ? tabTheme.textPrimary : tabTheme.textMuted
            font.pixelSize: 15
            font.weight: tabTheme.weightSemibold
        }

        Rectangle {
            id: tabUnderline
            x: 2
            anchors.bottom: tab.bottom
            width: tabLabel.width
            height: 2
            radius: 2
            color: tab.active ? tabTheme.accent : "transparent"
        }

        MouseArea {
            anchors.fill: tab
            hoverEnabled: false
            onClicked: tab.picked()
        }
    }

    /// MyQbzCardLite (KioskMyQBZ.slint:49-103). Note the cover square uses
    /// Radius.md — md, not sm, unlike every other kiosk card.
    component MyQbzCardLite: Rectangle {
        id: card

        property var item: ({})
        property real size: 150
        property bool navFocused: false
        signal clicked(string id)

        QbzTheme { id: cardTheme }

        readonly property string _id: card.item && card.item.id ? card.item.id : ""
        readonly property string _name: card.item && card.item.name ? card.item.name : ""
        readonly property string _label: card.item && card.item.label ? card.item.label : ""
        readonly property string _meta: card.item && card.item.meta ? card.item.meta : ""
        readonly property bool _hasCustom: card.item ? card.item.hasCustomCover === true : false
        readonly property string _customPath: card.item && card.item.customCoverPath
                                              ? card.item.customCoverPath : ""
        readonly property var _cellUrls: card.item && card.item.cellUrls ? card.item.cellUrls : []
        readonly property var _cellPaths: card.item && card.item.cellPaths ? card.item.cellPaths : []
        // Arm 2 is gated on the URL and sourced from the PATH, exactly as the
        // Slint gates on `url1` and mounts `cover1` (:75-79): gating on the
        // path would flip the card to the empty arm while the cover is still
        // downloading.
        readonly property bool _hasCell: card._cellUrls.length > 0 && card._cellUrls[0] !== ""
        readonly property string _cellPath: card._cellPaths.length > 0 ? card._cellPaths[0] : ""

        implicitWidth: card.size
        width: card.size
        implicitHeight: cardBody.implicitHeight
        border.width: card.navFocused ? 3 : 0
        border.color: cardTheme.accent
        radius: cardTheme.radiusSm
        color: card.navFocused
            ? Qt.rgba(cardTheme.accent.r, cardTheme.accent.g, cardTheme.accent.b, 0.12)
            : "transparent"

        Column {
            id: cardBody
            width: card.size
            spacing: 7

            Rectangle {
                id: coverTile
                width: card.size
                height: card.size
                radius: cardTheme.radiusMd
                color: cardTheme.surfaceElevated
                clip: true

                RoundedImage {
                    anchors.fill: coverTile
                    visible: card._hasCustom || card._hasCell
                    source: card._hasCustom ? card._customPath
                          : card._hasCell ? card._cellPath : ""
                    radius: coverTile.radius
                    fit: "crop"
                }
            }

            Text {
                width: card.size
                text: card._name
                color: cardTheme.textPrimary
                font.pixelSize: 14
                font.weight: cardTheme.weightSemibold
                elide: Text.ElideRight
                wrapMode: Text.NoWrap
                maximumLineCount: 1
            }

            Text {
                width: card.size
                // The separator carries a space on both sides
                // (KioskMyQBZ.slint:91). Both halves are translated in Rust.
                text: card._label + " · " + card._meta
                color: cardTheme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
                wrapMode: Text.NoWrap
                maximumLineCount: 1
            }
        }

        MouseArea {
            anchors.fill: card
            hoverEnabled: false
            onClicked: card.clicked(card._id)
        }
    }
}
