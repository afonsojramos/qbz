// CardMenu — the shared ⋯ menu surface for card overlays (promoted from
// LibraryView in phase 21): 33px rows, icon 15 + label 13, driven by an
// `entries` model of {label, icon, action}; emits picked(action).

import QtQuick
import com.blitzfc.qbz

QbzContextMenu {
    id: cmRoot

    property var entries: []
    signal picked(string action)

    QbzTheme { id: theme }

    Repeater {
        model: cmRoot.entries
        delegate: Rectangle {
            required property var modelData
            width: parent ? parent.width : 0
            height: 33
            radius: 5
            color: cmiArea.containsMouse ? theme.surfaceHover : "transparent"
            Row {
                anchors.fill: parent
                anchors.leftMargin: 8
                spacing: 8
                QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                Text {
                    height: parent.height
                    width: parent.width - 23
                    text: modelData.label
                    color: theme.textSecondary
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
            MouseArea {
                id: cmiArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: { cmRoot.close(); cmRoot.picked(modelData.action) }
            }
        }
    }
}
