// Root window: dark background + a Loader keyed on the bridge's `screen`
// property ("splash" | "login" | "shell"). Kicks off the Rust boot
// sequence once the QML tree is complete (the bridge registers its Qt
// thread handle in that first invokable, so every async UI hop lands).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

ApplicationWindow {
    id: window
    width: 1280
    height: 800
    minimumWidth: 800
    minimumHeight: 600
    visible: true
    title: "QBZ"
    color: "#0f0f0f"

    Component.onCompleted: QbzBridge.boot()

    Loader {
        id: screenLoader
        anchors.fill: parent
        active: QbzBridge.screen !== "splash"
        source: QbzBridge.screen === "login"
                ? "LoginScreen.qml"
                : (QbzBridge.screen === "shell" ? "ShellPlaceholder.qml" : "")
    }

    // Splash: shown until the silent session restore resolves.
    Rectangle {
        anchors.fill: parent
        color: "#0f0f0f"
        visible: QbzBridge.screen === "splash"

        Column {
            anchors.centerIn: parent
            spacing: 16
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                source: "assets/qbz-logo.png"
                width: 120
                height: 120
                fillMode: Image.PreserveAspectFit
            }
            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                running: true
            }
        }
    }
}
