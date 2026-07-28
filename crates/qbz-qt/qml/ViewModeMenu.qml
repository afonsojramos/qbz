// Now-Playing-view mode menu (PlayerBar.slint layout-menu, phase 18) —
// one QbzContextMenu shared by the full PlayerBar and the Small bar:
// New / Classic / Small / Large (with the current mode checked), then the
// inert window-mode rows (Miniplayer / Immersive / Kiosk — the Slint has
// them; the POC has no such surfaces, POC-NOTE).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

QbzContextMenu {
    id: root
    menuWidth: 196

    Repeater {
        model: [
            { "label": QbzBridge.tr("New", QbzBridge.trRev), "icon": "panel-left", "mode": 0 },
            { "label": QbzBridge.tr("Classic", QbzBridge.trRev), "icon": "panel-right-close", "mode": 1 },
            { "label": QbzBridge.tr("Small", QbzBridge.trRev), "icon": "rows-3", "mode": 2 },
            { "label": QbzBridge.tr("Large", QbzBridge.trRev), "icon": "layout-grid", "mode": 3 },
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
                    visible: QbzBridge.npbMode === modelData.mode
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
                    QbzBridge.npbSetMode(modelData.mode)
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
            { "label": QbzBridge.tr("Miniplayer", QbzBridge.trRev), "icon": "picture-in-picture-2" },
            { "label": QbzBridge.tr("Immersive", QbzBridge.trRev), "icon": "maximize-2" },
            { "label": QbzBridge.tr("Kiosk mode", QbzBridge.trRev), "icon": "hard-drive" },
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
