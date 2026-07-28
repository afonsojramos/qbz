// QbzNavButton — the 28px circular page/history chevron (Slint NavButton),
// consolidated in phase 22 from THREE identical copies (HomeView.NavBtn,
// SectionRail.RailNavBtn, HeaderBar.HeaderNavBtn). Contract: icon `name`,
// `btnEnabled` (dims to 0.4 + muted glyph, disarms), `clicked()`.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property string name: ""
    property bool btnEnabled: true
    signal clicked()

    QbzTheme { id: theme }

    width: 28
    height: 28
    radius: 14
    opacity: btnEnabled ? 1.0 : 0.4
    color: (nbArea.containsMouse && btnEnabled) ? theme.surfaceHover : theme.surfaceElevated
    QbzIcon {
        name: parent.name
        width: 15
        height: 15
        anchors.centerIn: parent
        tintName: parent.btnEnabled ? "primary" : "muted"
    }
    MouseArea {
        id: nbArea
        anchors.fill: parent
        enabled: parent.btnEnabled
        hoverEnabled: true
        cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: parent.clicked()
    }
}
