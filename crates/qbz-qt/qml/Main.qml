// Root window: dark background + a Loader keyed on the bridge's `screen`
// property ("splash" | "login" | "shell"). Kicks off the Rust boot
// sequence once the QML tree is complete (the bridge registers its Qt
// thread handle in that first invokable, so every async UI hop lands).
//
// Font: the Slint app bundles the Inter 18pt faces (app.slint); the same
// TTFs are embedded here via qrc and applied app-wide through the
// ApplicationWindow font (children inherit).

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

    FontLoader { id: interRegular; source: "assets/fonts/Inter_18pt-Regular.ttf" }
    FontLoader { id: interMedium; source: "assets/fonts/Inter_18pt-Medium.ttf" }
    FontLoader { id: interSemiBold; source: "assets/fonts/Inter_18pt-SemiBold.ttf" }
    FontLoader { id: interBold; source: "assets/fonts/Inter_18pt-Bold.ttf" }
    font.family: interRegular.status === FontLoader.Ready ? interRegular.name : "Sans Serif"

    Component.onCompleted: QbzBridge.boot()

    Loader {
        id: screenLoader
        anchors.fill: parent
        active: QbzBridge.screen !== "splash"
        source: QbzBridge.screen === "login"
                ? "LoginScreen.qml"
                : (QbzBridge.screen === "shell" ? "AppShell.qml" : "")
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
