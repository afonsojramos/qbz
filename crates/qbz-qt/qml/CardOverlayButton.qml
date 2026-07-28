// CardOverlayButton — the shared card hover-overlay button (the Slint
// per-card OverlayButton copies, homologated in the POC at
// LibraryView.LibOverlayBtn and promoted to shared in phase 21):
// primary = 44px white disc (black icon), ghost = 36px translucent disc
// (1.5px border); `active` tints the icon accent (follow/heart state).

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property string name: ""
    property bool primary: false
    property bool active: false
    signal clicked()

    width: primary ? 44 : 36
    height: primary ? 44 : 36
    radius: width / 2
    color: primary ? (obArea.containsMouse ? "#d6ffffff" : "#ffffff")
         : (obArea.containsMouse ? "#3dffffff" : "#24ffffff")
    border.width: primary ? 0 : 1.5
    border.color: "#ccffffff"
    QbzIcon {
        name: parent.name
        width: primary ? 18 : 16
        height: primary ? 18 : 16
        anchors.centerIn: parent
        tintName: parent.active ? "accent" : (parent.primary ? "black" : "primary")
    }
    property alias hovered: obArea.containsMouse
    MouseArea {
        id: obArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: parent.clicked()
    }
}
