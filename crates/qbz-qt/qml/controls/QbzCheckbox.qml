// QbzCheckbox (primitives/QbzCheckbox.slint) — 18px square, r4, accent fill
// with a black check when on, 1.5px muted border when off. Emits toggled();
// like the Slint original it never self-flips (the owner of the state does).
//
// Used by Settings > Local Library (folder selection) and the Plex library
// picker, i.e. every place the Slint uses its own QbzCheckbox.

import QtQuick
import "../theme"

Rectangle {
    id: root

    property bool checked: false
    signal toggled()

    QbzTheme { id: theme }

    width: 18
    height: 18
    radius: 4
    color: root.checked ? theme.accent : "transparent"
    border.width: root.checked ? 0 : 2
    border.color: theme.textMuted

    QbzIcon {
        anchors.centerIn: parent
        visible: root.checked
        name: "check"
        tintName: "black"
        width: 12
        height: 12
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled()
    }
}
