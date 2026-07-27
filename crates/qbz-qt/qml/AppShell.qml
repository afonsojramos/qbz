// App shell — the QML port of crates/qbz-ui/ui/shell/AppShell.slint's
// chrome: HeaderBar (top, 42px) / { Sidebar | content | queue column } /
// NowPlayingBarSmall (bottom). The recovery affordance and logout live in
// the header (offline badge flyout + app menu), like the Slint shell.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    color: theme.surfaceMain

    QbzTheme { id: theme }

    HeaderBar {
        id: header
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: theme.headerHeight
    }

    NowPlayingBarSmall {
        id: npb
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: theme.npbSmallHeight
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

    // Content frame — the view area between sidebar and queue column.
    Rectangle {
        id: contentFrame
        anchors.left: sidebar.right
        anchors.right: queueColumn.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        color: theme.surfaceMain
        clip: true

        Loader {
            anchors.fill: parent
            source: QbzBridge.currentView === "home" ? "HomeView.qml" : ""
        }
    }
}
