// QbzContextMenu — the shared popup surface for the app's ⋯ / context
// menus, with CORRECT positioning (phase 12):
//
//   - openAtCursor(item, localX, localY): context menus open AT THE
//     POINTER, like the Slint unified TrackContextMenu (TrackRow.slint:
//     `absolute-position - layout-offset + local-pointer`, clamped into
//     the row/window). Coordinates are mapped to the window and the menu
//     is parented to the Overlay, so a menu never renders inside the
//     triggering card/row.
//   - openBelowRight(item): button menus anchor under their trigger,
//     right edge aligned (the QbzSelect / sort-menu convention).
//   - Both clamp into the window with an 8px margin (Slint clamps the x
//     to `width - menu-width - 12`) so a menu never opens off-screen —
//     Popup does NOT do this on its own.
//
// The background matches ContextMenu.slint (surface-main, Radius.sm,
// 1px border-muted). Rows go in the default content (33px, icon 15 +
// label 13 per the existing per-site convention).
//
// Right-click parity: every ⋯ site also opens ITS menu on a right press
// through the same `openAtCursor(area, mouse.x, mouse.y)` — the button and
// the context gesture must never show different menus.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Popup {
    id: root

    parent: Overlay.overlay
    width: menuWidth
    padding: 5
    closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

    property int menuWidth: 196
    default property alias menuContent: col.data

    QbzTheme { id: theme }

    background: Rectangle {
        color: theme.surfaceMain
        radius: theme.radiusSm
        border.width: 1
        border.color: theme.borderMuted
    }
    contentItem: Column { id: col }

    // Final placement is computed BEFORE open() — the popup's implicit
    // height is already known (static content), and `Window.window` is
    // only reliably attached on the SOURCE item, not on the popup.
    function _place(gx, gy, win) {
        if (win) {
            x = Math.max(8, Math.min(gx, win.width - width - 8))
            y = Math.max(8, Math.min(gy, win.height - implicitHeight - 8))
        } else {
            x = gx
            y = gy
        }
        open()
    }

    /// Context menu at the pointer. `localX/localY` are the click coords
    /// inside `sourceItem` (the MouseArea that caught the click).
    function openAtCursor(sourceItem, localX, localY) {
        var g = sourceItem.mapToItem(null, localX, localY)
        _place(g.x, g.y, sourceItem.Window.window)
    }

    /// Button menu: below the trigger, right edges aligned.
    function openBelowRight(sourceItem) {
        var g = sourceItem.mapToItem(null, sourceItem.width - width, sourceItem.height + 4)
        _place(g.x, g.y, sourceItem.Window.window)
    }
}
