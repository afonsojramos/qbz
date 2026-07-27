// App shell — the QML port of crates/qbz-ui/ui/shell/AppShell.slint's
// chrome: HeaderBar (top, 42px) / { Sidebar | content frame | queue
// column } / NowPlayingBarSmall (bottom).
//
// The window chrome is surface-card throughout; the content area is a
// ROUNDED surface-main panel inset 8px left/right/bottom (0 top — it
// butts the header), the "Slack-style bezel on all four corners" of
// AppShell.slint:358-390. The recovery affordance and logout live in the
// header (offline badge flyout + app menu), like the Slint shell.
//
// POC-NOTE: the artwork-derived ambient background (AppearanceState
// "Ambient"/"Blurred art" modes) is not implemented — the Slint default
// is Off, which is what this static dark treatment matches.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    // TRANSPARENT — the chrome (header / sidebar / queue column / NPB)
    // paints every pixel between the header's rounded top corners and the
    // NPB's rounded bottom corners, so the window's rounded corners show
    // through (frameless translucent window).
    color: "transparent"

    // The host ApplicationWindow (custom chrome: drag / maximize / resize).
    property var hostWindow: null

    QbzTheme { id: theme }

    HeaderBar {
        id: header
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: theme.headerHeight
        hostWindow: root.hostWindow
        // Rounded top window corners (custom chrome); the bottom corners
        // are covered by the same-color surfaces below.
        radius: 12
    }

    NowPlayingBarSmall {
        id: npb
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: theme.npbSmallHeight
        // Rounded bottom window corners (custom chrome); the top corners
        // are covered by the same-color surfaces above.
        radius: 12
    }

    Sidebar {
        id: sidebar
        anchors.left: parent.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        // Animated 3-state width lives inside the component.
    }

    // Right-side queue column — a layout sibling of the content (it shrinks
    // the content, no overlay), animated 0 <-> 300 like the Slint AppShell
    // column (160ms ease-in-out).
    Rectangle {
        id: queueColumn
        anchors.right: parent.right
        anchors.top: header.bottom
        anchors.bottom: npb.top
        width: QbzBridge.queueOpen ? theme.queuePanelWidth : 0
        clip: true
        color: theme.surfaceCard

        Behavior on width {
            NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
        }

        QueuePanel {
            anchors.fill: parent
            visible: QbzBridge.queueOpen
        }
    }

    // Content frame — the rounded, inset surface-main panel (Radius.md,
    // 8px gaps left/right/bottom, flush to the header).
    Rectangle {
        id: contentFrame
        anchors.left: sidebar.right
        anchors.right: queueColumn.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        anchors.bottomMargin: 8
        radius: theme.radiusMd
        color: theme.surfaceMain
        border.width: 0
        clip: true

        Loader {
            anchors.fill: parent
            source: QbzBridge.currentView === "home" ? "HomeView.qml"
                : QbzBridge.currentView === "library" ? "LibraryView.qml" : ""
        }
    }
}
