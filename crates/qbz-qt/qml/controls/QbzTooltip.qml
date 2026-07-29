// Shared hover-tooltip overlay — the QML port of Slint's tooltip MECHANISM,
// not of one bubble: `shell/TooltipOverlay.slint` (bubble ABOVE the control,
// downward caret, clamped to the window) and `shell/SidebarTooltip.slint`
// (bubble to the RIGHT of a collapsed-rail row) are the same machine in Slint —
// a global state channel (`TooltipState` / `SidebarTooltipState`, state.slint:
// 4791 / 1893) that every hoverable control writes its text + anchor into, and
// ONE overlay mounted last in AppShell that renders it on top of everything.
//
// WHY AN OVERLAY AND NOT A `ToolTip`/`Popup` PER CALL SITE
//
// Two independent reasons, both measured:
//
//  1. QtQuick.Controls' `ToolTip` DOES NOT SIZE TO A CUSTOM contentItem.
//     Measured on Qt 6.11.1 with a probe (four variants, same contentItem):
//       Popup   { contentItem: Text {...} }  ->  implicitContentWidth = 157.8
//       ToolTip { contentItem: Text {...} }  ->  implicitContentWidth = 1
//     The Basic style's ToolTip binds
//       implicitWidth: max(implicitBackgroundWidth + insets,
//                          implicitContentWidth + padding)
//     so with implicitContentWidth stuck at 1 the popup collapses to a ~13x25
//     box while the Text — which a Popup never clips — keeps painting its full
//     178px. That is EXACTLY the reported defect: the label floats with no
//     background and a small detached background box sits beside it. Setting
//     `contentWidth`/`contentHeight` by hand papers over it; not being a
//     ToolTip at all removes it.
//  2. The rows that need a tooltip live inside a `clip: true` sidebar and
//     inside recycled delegates. A per-row popup would be one QQuickPopup per
//     playlist (the tree runs to hundreds) and would have to be torn down with
//     its delegate. The overlay is one Item at the shell root, and it stores
//     only NUMBERS + strings — never a reference to the anchoring item — so a
//     delegate can be destroyed under an open bubble without dangling.
//
// The overlay never takes the pointer (no MouseArea, `enabled: false`), so it
// cannot steal the hover that is keeping it open — the same rule
// ArtPreviewOverlay.qml documents.
//
// PLACEMENTS (both flavours keep their own Slint numbers, they are not one
// style with two anchors):
//   showRight()  SidebarTooltip.slint — surface-elevated, Radius.sm, 1px
//                border-muted, 10/10/5/5 padding, 12px/w500 text-primary,
//                x = row.right + 6, vertically centred on the row.
//   showAbove()  TooltipOverlay.slint — surface-elevated, radius 4, no border,
//                9/9/5/5 padding, 11px/medium text-primary, downward caret,
//                y = anchor.top - height - 9, centre clamped 8px from the
//                window edges with the caret still pointing at the control.
//
// DELIBERATE DEVIATIONS FROM SLINT, both stated because they are visible:
//   - MAX WIDTH + ELISION. Neither Slint bubble caps its width; a 200-character
//     playlist name would run off the window. `maxWidth` (320 default) caps it
//     and the label elides. Set maxWidth: 0 for the uncapped Slint behaviour.
//   - SHADOW. Slint draws `drop-shadow-blur: 16px` / `10px`. Qt Quick has no
//     shadow primitive on the software path (QtQuick.Effects renders NOTHING —
//     see theme/RoundedImage.qml), so the port's usual approximation is used:
//     one offset rectangle behind the bubble, exactly like
//     SidebarNowPlayingDock.qml does for the cover.

import QtQuick
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // Purely decorative — never take a click, never take the hover that is
    // keeping the bubble open.
    enabled: false

    // ---- State (the QML stand-in for Slint's TooltipState globals) --------
    // `ownerKey` is the race-safe owner id Slint carries in
    // SidebarTooltipState.id: a row that loses the pointer only clears the
    // bubble if it still owns it, so sliding straight from one row to the next
    // never blanks it (Sidebar.slint:199-203).
    property string ownerKey: ""
    property string label: ""
    // 0 = right of the anchor (sidebar rail), 1 = above it (caret bubble).
    property int placement: 0
    // Anchor rectangle, in THIS overlay's coordinates. Captured as plain
    // numbers on show — never a reference to the anchoring Item, so a recycled
    // or destroyed delegate cannot dangle here.
    property real anchorX: 0
    property real anchorY: 0
    property real anchorW: 0
    property real anchorH: 0
    // 0 disables the cap (Slint's own behaviour).
    property int maxWidth: 320

    readonly property bool shown: ownerKey !== "" && label !== ""

    // ---- API --------------------------------------------------------------
    // `item` is any Item; its bounds are mapped into this overlay. `key` is the
    // owner id used by hide().
    function showRight(item, key, text) {
        if (!item || !text)
            return
        root._capture(item)
        root.placement = 0
        root.label = text
        root.ownerKey = key
    }
    function showAbove(item, key, text) {
        if (!item || !text)
            return
        root._capture(item)
        root.placement = 1
        root.label = text
        root.ownerKey = key
    }
    // Race-safe close: only the owner may clear the bubble.
    function hide(key) {
        if (root.ownerKey === key)
            root.ownerKey = ""
    }
    // Unconditional close — for the events that invalidate the anchor itself
    // (the sidebar leaving the mini state, a flyout taking the same slot, a
    // view change).
    function hideAll() {
        root.ownerKey = ""
    }

    function _capture(item) {
        const p = item.mapToItem(root, 0, 0)
        root.anchorX = p.x
        root.anchorY = p.y
        root.anchorW = item.width
        root.anchorH = item.height
    }

    // ---- Geometry ---------------------------------------------------------
    // The bubble is sized by its LABEL, which is the whole point: `bubbleW`
    // reads the Text's natural implicitWidth (NoWrap + ElideRight leaves
    // implicitWidth at the unelided width, so assigning the Text a narrower
    // width below is not a binding loop) and the cap only ever shrinks it.
    readonly property real padH: placement === 0 ? 10 : 9
    readonly property real padV: 5
    readonly property real bubbleW: Math.min(
        tipText.implicitWidth + 2 * padH,
        maxWidth > 0 ? maxWidth : Number.MAX_VALUE)
    readonly property real bubbleH: tipText.implicitHeight + 2 * padV

    // Right placement (SidebarTooltip.slint:17-19): +6px off the row's right
    // edge, vertically centred on the row. Flips to the row's LEFT when the
    // bubble would leave the window — Slint never clamps this one because its
    // rail is always at x=0 with the whole window to its right; the flip costs
    // nothing and keeps a narrow window honest.
    readonly property real rightX: (anchorX + anchorW + 6 + bubbleW <= root.width - 8)
        ? anchorX + anchorW + 6
        : Math.max(8, anchorX - bubbleW - 6)
    readonly property real rightY: Math.max(8, Math.min(
        root.height - bubbleH - 8,
        anchorY + Math.round((anchorH - bubbleH) / 2)))

    // Above placement (TooltipOverlay.slint:17-26): centred on the control,
    // clamped 8px from both window edges, 9px of air over the control.
    readonly property real aboveCx: anchorX + anchorW / 2
    readonly property real aboveX: Math.max(8, Math.min(
        Math.max(8, root.width - bubbleW - 8), aboveCx - bubbleW / 2))
    readonly property real aboveY: anchorY - bubbleH - 9

    // ---- The bubble -------------------------------------------------------
    Item {
        id: bubble
        visible: root.shown
        x: Math.round(root.placement === 0 ? root.rightX : root.aboveX)
        y: Math.round(root.placement === 0 ? root.rightY : root.aboveY)
        width: Math.ceil(root.bubbleW)
        height: Math.ceil(root.bubbleH)

        // Shadow approximation (see the header): one offset rect, same radius.
        Rectangle {
            x: 0
            y: root.placement === 0 ? 4 : 2
            width: parent.width
            height: parent.height
            radius: root.placement === 0 ? theme.radiusSm : 4
            color: "#66000000"
        }

        Rectangle {
            anchors.fill: parent
            radius: root.placement === 0 ? theme.radiusSm : 4
            color: theme.surfaceElevated
            // Only the sidebar bubble is bordered (SidebarTooltip.slint:24-25);
            // TooltipOverlay.slint has none.
            border.width: root.placement === 0 ? 1 : 0
            border.color: theme.borderMuted

            Text {
                id: tipText
                x: root.padH
                y: root.padV
                width: Math.max(0, parent.width - 2 * root.padH)
                text: root.label
                color: theme.textPrimary
                font.pixelSize: root.placement === 0 ? 12 : 11
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
            }
        }

        // Downward caret, above-placement only. Slint draws a Path
        // ("M 0 0 L 5 6 L 10 0 Z", TooltipOverlay.slint:56-64) and keeps it
        // pointing at the REAL control centre even when the bubble is clamped.
        // Canvas is this port's polygon primitive (Canvas.Immediate — the
        // Cooperative/Threaded render strategies segfault against list
        // recycling; see theme/RoundedImage.qml).
        Canvas {
            id: caret
            visible: root.placement === 1
            width: 10
            height: 6
            x: Math.round(Math.max(4, Math.min(parent.width - width - 4,
                                               root.aboveCx - bubble.x - width / 2)))
            y: parent.height - 1
            renderTarget: Canvas.Image
            renderStrategy: Canvas.Immediate
            // The fill has to follow the theme, and a Canvas does not repaint
            // on a colour binding by itself.
            property color fill: theme.surfaceElevated
            onFillChanged: caret.requestPaint()
            onVisibleChanged: if (visible) caret.requestPaint()
            onPaint: {
                var ctx = caret.getContext("2d")
                if (!ctx)
                    return
                ctx.reset()
                ctx.clearRect(0, 0, caret.width, caret.height)
                ctx.fillStyle = caret.fill
                ctx.beginPath()
                ctx.moveTo(0, 0)
                ctx.lineTo(caret.width / 2, caret.height)
                ctx.lineTo(caret.width, 0)
                ctx.closePath()
                ctx.fill()
            }
        }
    }
}
