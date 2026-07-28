// The app-wide selection checkbox (LocalLibraryView.slint:486-515): a 13px
// circle, accent-filled when on, 1.5px muted ring when off, 8px white check
// glyph — or a minus glyph for the folder tri-state "partial".
//
// Slint declares this inline inside TreeRow; the Qt port also needs it on
// the album cards and the track rows in multi-select, so it is its own file
// (three call sites, one shape).

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    property bool on: false
    property bool partial: false
    property int diameter: 13
    signal toggled()

    QbzTheme { id: theme }

    width: diameter
    height: diameter
    radius: diameter / 2
    color: (on || partial) ? theme.accent : "transparent"
    border.width: (on || partial) ? 0 : 1.5
    border.color: checkArea.containsMouse ? theme.textPrimary : theme.textMuted

    QbzIcon {
        visible: root.on || root.partial
        name: root.partial ? "minus" : "check"
        width: Math.round(root.diameter * 0.62)
        height: Math.round(root.diameter * 0.62)
        anchors.centerIn: parent
        tintName: "primary"
    }
    MouseArea {
        id: checkArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled()
    }
}
