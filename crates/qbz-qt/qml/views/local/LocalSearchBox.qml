// Local Library search input — the ONE box the whole surface uses.
//
// Slint has three near-identical inputs on this view (LocalLibraryView.slint:
// the toolbar ExpandableSearch `sm`, the tree-rail header input at :1724 and
// the folder-detail input at :1984). They differ only in width, left padding
// and glyph size, so this is one component with those three as properties
// instead of three copies.
//
// Numbers: 30px tall, radius 6, surface-card fill, 1px border that turns
// accent on focus, 12px text, muted placeholder (Slint :1726-1771).

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    property string placeholder: ""
    property alias text: input.text
    property int boxWidth: 200
    property int iconSize: 13
    property int leftPad: 9
    property int rightPad: 8
    /// Elevated fill for the toolbar variant; the rail/detail inputs use
    /// surface-card (Slint :1728).
    property bool elevated: false
    signal edited(string value)

    QbzTheme { id: theme }

    width: boxWidth
    height: 30
    radius: 6
    color: elevated ? theme.surfaceElevated : theme.surfaceCard
    border.width: 1
    border.color: input.activeFocus ? theme.accent : theme.borderSubtle
    clip: true

    Row {
        anchors.fill: parent
        anchors.leftMargin: root.leftPad
        anchors.rightMargin: root.rightPad
        spacing: 6

        QbzIcon {
            name: "search"
            width: root.iconSize
            height: root.iconSize
            anchors.verticalCenter: parent.verticalCenter
            tintName: "muted"
        }
        Item {
            width: parent.width - root.iconSize - 6
            height: parent.height
            clip: true
            TextInput {
                id: input
                anchors.fill: parent
                color: theme.textPrimary
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                clip: true
                selectByMouse: true
                onTextEdited: root.edited(text)
            }
            Text {
                visible: input.text === ""
                anchors.fill: parent
                text: root.placeholder
                color: theme.textMuted
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
    }
}
