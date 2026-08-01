// Now-Playing-view mode menu (PlayerBar.slint layout-menu, phase 18) —
// one QbzContextMenu shared by the full PlayerBar and the Small bar:
// New / Classic / Small / Large (with the current mode checked), then the
// inert window-mode rows (Miniplayer / Immersive / Kiosk — the Slint has
// them; the POC has no such surfaces, POC-NOTE).

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

    // Divider + the window-mode rows (Miniplayer / Immersive / Kiosk —
    // present in the Slint menu; inert in the POC (no such surfaces).
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
        model: [
            { "label": QbzSession.tr("Miniplayer", QbzSession.trRev), "icon": "picture-in-picture-2" },
            { "label": QbzSession.tr("Immersive", QbzSession.trRev), "icon": "maximize-2" },
            { "label": QbzSession.tr("Kiosk mode", QbzSession.trRev), "icon": "hard-drive" },
        ]
        delegate: Rectangle {
            required property var modelData
            width: parent ? parent.width : 0
            height: 33
            radius: 5
            opacity: 0.45
            color: "transparent"
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
            // Inert (POC-NOTE): no miniplayer/immersive/kiosk surfaces.
        }
    }
}
