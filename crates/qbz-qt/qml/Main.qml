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
    // Custom chrome (phase 7): frameless + translucent so the window draws
    // its own rounded corners, 1:1 with the Slint custom-chrome look.
    flags: Qt.Window | Qt.FramelessWindowHint
    color: "transparent"

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
        // Hand the host window down for drag/maximize/resize (custom chrome).
        onLoaded: if (screenLoader.item) screenLoader.item.hostWindow = window
    }

    // Rounded-window hairline frame (app.slint's no-frame 1px edge) —
    // paints at the very rim, over everything, clipped by the corners.
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        radius: 12
        border.width: 1
        border.color: "#14ffffff"
        visible: window.visibility !== Window.Maximized && window.visibility !== Window.FullScreen
    }

    // Edge/corner resize grips (custom chrome — the compositor draws no
    // border). 6px, invisible.
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        onPressed: window.startSystemResize(Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        onPressed: window.startSystemResize(Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 6
        cursorShape: Qt.SizeVerCursor
        onPressed: window.startSystemResize(Qt.TopEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 6
        cursorShape: Qt.SizeVerCursor
        onPressed: window.startSystemResize(Qt.BottomEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.RightEdge)
    }

    // Splash (SplashScreen.slint): the same 720px dark card as the login
    // screen while the silent session restore resolves.
    Rectangle {
        anchors.fill: parent
        color: "#0f0f0f"
        radius: 12
        visible: QbzBridge.screen === "splash"

        Rectangle {
            anchors.centerIn: parent
            width: 720
            height: splashColumn.height + 104
            radius: 16
            color: "#1a1a1a"

            Column {
                id: splashColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 52
                spacing: 0
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "assets/qbz-logo.png"
                    width: 140
                    height: 140
                    fillMode: Image.PreserveAspectFit
                }
                Item { width: 1; height: 8 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "QBZ"
                    color: "#ffffff"
                    font.pixelSize: 29
                    font.weight: Font.DemiBold
                    font.letterSpacing: 8
                }
                Item { width: 1; height: 2 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "QOBUZ™ PLAYER"
                    color: "#888888"
                    font.pixelSize: 15
                    font.letterSpacing: 4
                }
                Item { width: 1; height: 32 }
                QbzSpinner {
                    anchors.horizontalCenter: parent.horizontalCenter
                    size: 32
                }
            }
        }
    }
}
