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
import "theme"

ApplicationWindow {
    id: window
    width: 1280
    height: 800
    minimumWidth: 800
    minimumHeight: 600
    visible: true
    title: "QBZ"
    // Custom chrome (phase 7/12): frameless but OPAQUE — the phase-7
    // translucent window was a misread: the Slint MAIN window keeps an
    // OPAQUE swapchain (only the miniplayer blends; crates/qbz/src/main.rs
    // set_surface_prefers_transparent + the Cargo.toml patch comment), and
    // the rounded corners in the Slint screenshots come from the
    // COMPOSITOR, not the app — app.slint's root is opaque surface-main
    // with square corners and a square 1px hairline frame. The system
    // titlebar is the `use_system_title_bar` pref (ui_prefs.json; applied
    // at startup, restart semantics like Slint).
    flags: QbzShell.systemTitleBar ? Qt.Window : (Qt.Window | Qt.FramelessWindowHint)
    color: "#1a1a1a"

    FontLoader { id: interRegular; source: "assets/fonts/Inter_18pt-Regular.ttf" }
    FontLoader { id: interMedium; source: "assets/fonts/Inter_18pt-Medium.ttf" }
    FontLoader { id: interSemiBold; source: "assets/fonts/Inter_18pt-SemiBold.ttf" }
    FontLoader { id: interBold; source: "assets/fonts/Inter_18pt-Bold.ttf" }
    font.family: interRegular.status === FontLoader.Ready ? interRegular.name : "Sans Serif"

    // Phase 23: every domain singleton boots (registers its Qt-thread
    // hop; QbzSession.boot additionally fires the app boot sequence).
    Component.onCompleted: {
        QbzSession.boot()
        QbzShell.boot()
        QbzPlayer.boot()
        QbzQueue.boot()
        QbzHome.boot()
        QbzBridge.boot()
    }







    Loader {
        id: screenLoader
        anchors.fill: parent
        active: QbzSession.screen !== "splash"
        source: QbzSession.screen === "login"
                ? "LoginScreen.qml"
                : (QbzSession.screen === "shell" ? "shell/AppShell.qml" : "")
        // Hand the host window down for drag/maximize/resize (custom chrome).
        onLoaded: if (screenLoader.item) screenLoader.item.hostWindow = window
    }

    // Frameless hairline (app.slint's no-frame 1px edge) — paints at the
    // very rim, over everything. SQUARE (the app draws no corner rounding).
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: 1
        border.color: "#14ffffff"
        visible: !QbzShell.systemTitleBar
            && window.visibility !== Window.Maximized && window.visibility !== Window.FullScreen
    }

    // Edge/corner resize grips (custom chrome — the compositor draws no
    // border). 6px, invisible.
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 6
        cursorShape: Qt.SizeVerCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 6
        cursorShape: Qt.SizeVerCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.RightEdge)
    }

    // Splash (SplashScreen.slint): the same 720px dark card as the login
    // screen while the silent session restore resolves.
    Rectangle {
        anchors.fill: parent
        color: "#0f0f0f"
        visible: QbzSession.screen === "splash"

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
