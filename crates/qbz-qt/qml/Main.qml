import QtQuick
import QtQuick.Controls

ApplicationWindow {
    visible: true
    width: 960
    height: 600
    title: "qbz-qt — toolchain gate"

    Rectangle {
        anchors.fill: parent
        color: "#191922"

        Text {
            anchors.centerIn: parent
            text: "qbz-qt phase 0 gate"
            color: "#e8e8ef"
            font.pixelSize: 20
        }
    }
}
