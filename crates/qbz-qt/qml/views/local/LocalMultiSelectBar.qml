// Bulk action bar (primitives/MultiSelectBar.slint, mounted by
// LocalLibraryView.slint:1239 for Albums and :1406 for Tracks).
//
// 44px tall, radius 8, surface-elevated: "N selected" on the left, then one
// 34x30 icon button per action — dimmed to 0.4 and disarmed while the action
// needs a selection and there is none, red-bordered when it is a danger
// action. ADR-008: bordered/hover buttons, not pills.
//
// The Slint puts the action label in the shared tooltip bubble; the Qt port
// has no tooltip channel yet, so the label rides Qt's own ToolTip.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    /// [{ id, label, icon, danger, needsSelection }]
    property var actions: []
    property int selectedCount: 0
    signal action(string id)

    QbzTheme { id: theme }

    height: 44
    radius: 8
    color: theme.surfaceElevated

    Row {
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 10
        spacing: 6

        Text {
            width: parent.width - actionRow.width - 6
            height: parent.height
            text: root.selectedCount + " "
                + QbzSession.tr("selected", QbzSession.trRev)
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Row {
            id: actionRow
            height: parent.height
            spacing: 6
            Repeater {
                model: root.actions
                delegate: Rectangle {
                    id: btn
                    required property var modelData
                    readonly property bool armed: !modelData.needsSelection
                        || root.selectedCount > 0
                    width: 34
                    height: 30
                    anchors.verticalCenter: parent.verticalCenter
                    radius: theme.radiusSm
                    opacity: armed ? 1.0 : 0.4
                    color: modelData.danger
                        ? (btnArea.containsMouse ? theme.dangerHover : "transparent")
                        : (btnArea.containsMouse ? theme.surfaceHover : "transparent")
                    border.width: modelData.danger ? 1 : 0
                    border.color: theme.dangerBorder
                    QbzIcon {
                        name: btn.modelData.icon
                        width: 16
                        height: 16
                        anchors.centerIn: parent
                        tintName: btn.modelData.danger ? "warning"
                            : btnArea.containsMouse ? "textPrimary" : "secondary"
                    }
                    MouseArea {
                        id: btnArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: btn.armed ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (btn.armed) root.action(btn.modelData.id)
                        ToolTip.visible: containsMouse
                        ToolTip.text: btn.modelData.label
                        ToolTip.delay: 400
                    }
                }
            }
        }
    }
}
