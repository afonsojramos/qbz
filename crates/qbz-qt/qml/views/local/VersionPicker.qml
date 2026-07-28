// Local album version picker (album/LocalAlbumView.slint:41 VersionPicker) —
// shown only when an album has several PHYSICAL copies (distinct source
// folders), so two copies never merge into a duplicated track list.
//
// Sized like QbzSelect `sm` (30px, radius 6, 10px left pad) so it reads as
// one of the small selects rather than towering over them: source icon +
// label + chevron, with a 280px dropdown that repeats the icon and ticks the
// current entry.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Rectangle {
    id: root

    /// [{ label, source }]
    property var versions: []
    property int current: 0
    signal picked(int index)

    QbzTheme { id: theme }

    readonly property var currentVersion: versions[current] || ({})

    height: 30
    width: row.width
    radius: 6
    color: pickArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated

    Row {
        id: row
        height: parent.height
        leftPadding: 10
        rightPadding: 8
        spacing: 7
        SourceIcon {
            anchors.verticalCenter: parent.verticalCenter
            kind: root.currentVersion.source || "local"
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.currentVersion.label || ""
            color: theme.textSecondary
            font.pixelSize: theme.fontLegal
        }
        QbzIcon {
            anchors.verticalCenter: parent.verticalCenter
            name: "chevron-down"
            width: 12
            height: 12
            tintName: "muted"
        }
    }
    MouseArea {
        id: pickArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: menu.openBelowRight(pickArea)
    }

    QbzContextMenu {
        id: menu
        menuWidth: 280
        Repeater {
            model: root.versions
            delegate: Rectangle {
                id: opt
                required property var modelData
                required property int index
                width: parent ? parent.width : 0
                height: 30
                radius: 6
                color: index === root.current ? theme.surfaceElevated
                     : optArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8
                    SourceIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        kind: opt.modelData.source || "local"
                    }
                    Text {
                        width: parent.width - 22 - (opt.index === root.current ? 20 : 0)
                        height: parent.height
                        text: opt.modelData.label || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontLegal
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    QbzIcon {
                        visible: opt.index === root.current
                        anchors.verticalCenter: parent.verticalCenter
                        name: "check"
                        width: 12
                        height: 12
                        tintName: "accent"
                    }
                }
                MouseArea {
                    id: optArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { menu.close(); root.picked(opt.index) }
                }
            }
        }
    }
}
