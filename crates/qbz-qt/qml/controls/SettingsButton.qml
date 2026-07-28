// Settings SecondaryButton (IntegrationsSettings.slint:27 /
// AppearanceSettings.slint:83) — min 160px (compact variant 34px square for
// icon buttons), 34px tall, bordered, optional danger red (#e0564f ≈
// theme.danger), busy spinner text.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    property string text: ""
    property string iconName: ""
    property bool danger: false
    property bool busy: false
    property bool enabled: true
    signal clicked()

    QbzTheme { id: theme }

    width: Math.max(iconName !== "" && text === "" ? 34 : 160, row.implicitWidth + 24)
    height: 34
    radius: theme.radiusSm
    border.width: 1
    border.color: danger ? theme.danger : theme.borderSubtle
    color: enabled && btnArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
    opacity: enabled ? 1.0 : 0.4

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 8
        QbzIcon {
            visible: iconName !== ""
            name: iconName
            width: 15
            height: 15
            anchors.verticalCenter: parent.verticalCenter
            tintName: danger ? "favorite" : "secondary"
        }
        Text {
            visible: text !== ""
            text: parent.parent.text
            color: parent.parent.danger ? theme.danger : theme.textPrimary
            font.pixelSize: theme.fontBody
            font.weight: theme.weightMedium
            verticalAlignment: Text.AlignVCenter
        }
    }
    MouseArea {
        id: btnArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: parent.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (parent.enabled) parent.clicked()
    }
}
