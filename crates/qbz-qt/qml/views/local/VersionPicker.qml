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

    /// [{ version, trackCount, quality, source }]
    property var versions: []
    property int current: 0
    signal picked(int index)

    QbzTheme { id: theme }

    readonly property var currentVersion: versions[current] || ({})

    function optionText(version) {
        var parts = []
        if ((version.version || "") !== "")
            parts.push(version.version)
        parts.push((version.trackCount || 0) + " "
                   + QbzSession.tr("tracks", QbzSession.trRev))
        if ((version.quality || "") !== "")
            parts.push(version.quality)
        return parts.join(" · ")
    }

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
            // A dense ROW: the media marks draw monochrome and tinted, like the
            // hard-drive beside them. Colour logos are for cards — a list of
            // them fights the text it labels.
            mono: true
            // No size overrides on purpose: LocalAlbumView.slint:24-26 draws
            // ALL THREE kinds at a flat 14px here, which is SourceIcon's
            // default (the row glyphs are the ones that grow the marks).
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.optionText(root.currentVersion)
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
        menuWidth: 380
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
                        mono: true
                    }
                    Text {
                        width: parent.width - 22 - (opt.index === root.current ? 20 : 0)
                        height: parent.height
                        text: root.optionText(opt.modelData)
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
