// SettingRow (settings/SettingRow.slint) — extracted from SettingsView.qml
// in phase 19. 52px (64 with a description); label 15 medium + description
// 12 muted on the left (opacity .45 when disabled), the control flush right.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    property string label: ""
    property string description: ""
    property bool rowEnabled: true
    default property alias control: controlHost.data

    QbzTheme { id: theme }

    width: parent ? parent.width : 0
    height: description === "" ? 52 : 64

    Column {
        anchors.left: parent.left
        anchors.right: controlHost.left
        anchors.rightMargin: 24
        anchors.verticalCenter: parent.verticalCenter
        spacing: 3
        opacity: rowEnabled ? 1.0 : 0.45
        Text {
            width: parent.width
            text: label
            color: theme.textPrimary
            font.pixelSize: theme.fontBody
            font.weight: theme.weightMedium
            elide: Text.ElideRight
        }
        Text {
            visible: description !== ""
            width: parent.width
            text: description
            color: theme.textMuted
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }
    }
    Item {
        id: controlHost
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: childrenRect.width
        height: childrenRect.height
    }
}
