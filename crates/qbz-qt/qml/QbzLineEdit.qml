// QbzLineEdit (std-widgets LineEdit, settings styling) — extracted from
// SettingsView.qml in phase 19. 240px / 34px elevated r8 bordered input;
// commits on Enter AND on focus loss (the Tauri onchange semantics —
// PlaybackSettings.slint).

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property string text: ""
    property string placeholder: ""
    property bool isPassword: false
    signal committed(string value)

    QbzTheme { id: theme }

    width: 240
    height: 34
    radius: theme.radiusSm
    border.width: 1
    border.color: theme.borderSubtle
    color: theme.surfaceElevated

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        color: theme.textPrimary
        font.pixelSize: theme.fontBody
        verticalAlignment: Text.AlignVCenter
        clip: true
        echoMode: parent.isPassword ? TextInput.Password : TextInput.Normal
        text: parent.text
        onAccepted: parent.committed(text)
        onActiveFocusChanged: if (!activeFocus) parent.committed(text)
        // External republish (e.g. Reset) re-seeds the field while it is
        // not being edited.
        Binding {
            target: input
            property: "text"
            value: input.parent.text
            when: !input.activeFocus
        }
    }
    Text {
        visible: input.text === ""
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        text: placeholder
        color: theme.textMuted
        font.pixelSize: theme.fontBody
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
}
