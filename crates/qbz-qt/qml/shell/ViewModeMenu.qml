// Now-Playing-view mode menu (PlayerBar.slint layout-menu, phase 18) —
// one QbzContextMenu shared by the full PlayerBar and the Small bar:
// New / Classic / Small / Large (with the current mode checked), then the
// window-mode rows. Immersive is LIVE (2026-08-02 immersive-port §7, B2);
// Miniplayer / Kiosk stay inert (kiosk goes live in the kiosk contract §8).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

QbzContextMenu {
    id: root
    menuWidth: 196

    // The theme tokens. Declared HERE even though the BASE type
    // (controls/QbzContextMenu.qml) already has a `QbzTheme { id: theme }`:
    // a QML id belongs to the document that declares it, and a derived
    // document is a DIFFERENT scope — the base's context is created as a
    // child of this one, never as its parent, so `theme` was resolving to
    // nothing here and every binding below threw a silent ReferenceError
    // (leaving the row Rectangles at their default WHITE fill). The four
    // sibling menus built on the same base — CardMenu, AudioSettingsMenu,
    // PmFolderMenu, SidebarRowMenu — all declare their own; this file was
    // the lone outlier.
    QbzTheme { id: theme }

    Repeater {
        model: [
            { "label": QbzSession.tr("New", QbzSession.trRev), "icon": "panel-left", "mode": 0 },
            { "label": QbzSession.tr("Classic", QbzSession.trRev), "icon": "panel-right-close", "mode": 1 },
            { "label": QbzSession.tr("Small", QbzSession.trRev), "icon": "rows-3", "mode": 2 },
            { "label": QbzSession.tr("Large", QbzSession.trRev), "icon": "layout-grid", "mode": 3 },
        ]
        delegate: Rectangle {
            required property var modelData
            width: parent ? parent.width : 0
            height: 33
            radius: 5
            color: miArea.containsMouse ? theme.surfaceHover : "transparent"
            Row {
                anchors.fill: parent
                anchors.leftMargin: 8
                spacing: 8
                QbzIcon {
                    name: modelData.icon
                    width: 15
                    height: 15
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: "secondary"
                }
                Text {
                    height: parent.height
                    width: parent.width - 23 - 22
                    text: modelData.label
                    color: theme.textSecondary
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
                QbzIcon {
                    visible: QbzShell.npbMode === modelData.mode
                    name: "check"
                    width: 13
                    height: 13
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: "accent"
                }
            }
            MouseArea {
                id: miArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    root.close()
                    QbzShell.npbSetMode(modelData.mode)
                }
            }
        }
    }

    // Divider + the window-mode rows (Miniplayer / Immersive / Kiosk).
    // Immersive is live (below); the other two stay inert.
    Item {
        width: parent ? parent.width : 0
        height: 7
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            height: 1
            color: theme.borderSubtle
        }
    }
    Repeater {
        // Window-mode rows. `live` flips per-row inside the shared delegate
        // (2026-08-02 immersive-port §7); Immersive went live in that
        // contract's B2, Kiosk in the 2026-08-02-kiosk-port §8.1. Miniplayer
        // remains the one inert row (sanctioned, kiosk contract §9.2).
        //
        // Each row carries its own `action` because the delegate's MouseArea
        // used to call QbzImmersive.openFromMenu() unconditionally: flipping
        // any other row's `live` would have made it open Immersive.
        //
        // The Kiosk label mirrors the profile, 1:1 with the reference
        // (`shell/PlayerBar.slint:797`): in kiosk it reads "Desktop mode",
        // because the row is the way back out.
        model: [
            { "label": QbzSession.tr("Miniplayer", QbzSession.trRev), "icon": "picture-in-picture-2", "live": false, "action": "miniplayer" },
            { "label": QbzSession.tr("Immersive", QbzSession.trRev), "icon": "maximize-2", "live": true, "action": "immersive" },
            { "label": QbzShell.kioskProfile
                       ? QbzSession.tr("Desktop mode", QbzSession.trRev)
                       : QbzSession.tr("Kiosk mode", QbzSession.trRev),
              "icon": "hard-drive", "live": true, "action": "kiosk" },
        ]
        delegate: Rectangle {
            required property var modelData
            width: parent ? parent.width : 0
            height: 33
            radius: 5
            opacity: modelData.live ? 1.0 : 0.45
            color: (modelData.live && modeArea.containsMouse)
                   ? theme.surfaceHover : "transparent"
            Row {
                anchors.fill: parent
                anchors.leftMargin: 8
                spacing: 8
                QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                Text {
                    height: parent.height
                    text: modelData.label
                    color: theme.textSecondary
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                }
            }
            MouseArea {
                id: modeArea
                anchors.fill: parent
                enabled: modelData.live
                hoverEnabled: true
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: {
                    root.close()
                    if (modelData.action === "immersive")
                        QbzImmersive.openFromMenu()
                    else if (modelData.action === "kiosk")
                        QbzSession.toggleProfile()
                    // "miniplayer" has live: false, so `enabled` above keeps
                    // this handler unreachable for it.
                }
            }
        }
    }
}
