// Left navigation sidebar — QML port of crates/qbz-ui/ui/shell/Sidebar.slint.
//
// Three states (ShellState.sidebar-state): 0 = open 240px (icon + label),
// 1 = mini 64px (icons only), 2 = closed 0px. Width animates 160ms
// ease-in-out; the header's panel-left button cycles (QbzBridge.cycleSidebar).
//
// Top-level section nav rows (Discover / Library / Local Library / My QBZ)
// replicate SidebarNavRow: 34px rows, radius 6, 16px icons, 13px/w500
// labels, surface-hover on hover, Discover + Library HIDDEN while offline
// (ADR-010 mount-site gating).
//
// POC-NOTE: in the Slint app these rows open dropdown flyout menus; the
// flyouts are out of scope — rows here navigate (only "home" exists, so
// only Discover is live) and carry the SidebarDirectRow active treatment
// (surface-hover bg + primary text/icon) for the current section.
// POC-NOTE: the playlist/folder tree below the nav is out of scope; the
// "PLAYLISTS" header + toolbar render for parity and the Slint empty state
// shows ("No playlists yet.").

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root

    property bool mini: QbzBridge.sidebarState === 1
    // The current section — Discover (home) is the only real view.
    property string activeNav: "discover"

    QbzTheme { id: theme }

    width: QbzBridge.sidebarState === 2 ? 0
         : QbzBridge.sidebarState === 1 ? theme.sidebarMiniWidth
         : theme.sidebarOpenWidth
    color: theme.surfaceCard
    // Square edges; clip keeps content from spilling while the width
    // animates (same as the Slint root).
    clip: true

    Behavior on width {
        NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
    }

    // One section-nav row (SidebarNavRow / SidebarDirectRow metrics).
    component NavRow: Rectangle {
        id: navRow
        property string navId: ""
        property string name: ""
        property string label: ""
        property bool active: root.activeNav === navId
        signal clicked()

        width: parent ? parent.width : 0
        height: 34
        radius: 6
        color: (navArea.containsMouse || active) ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: root.mini ? 0 : 8
            anchors.rightMargin: root.mini ? 0 : 8
            spacing: 10

            Item {
                width: root.mini ? parent.width : 16
                height: parent.height
                QbzIcon {
                    name: navRow.name
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: (navArea.containsMouse || navRow.active)
                          ? "primary" : "secondary"
                }
            }
            Text {
                visible: !root.mini
                height: parent.height
                width: parent.width - (root.mini ? 0 : 26)
                text: navRow.label
                color: (navArea.containsMouse || navRow.active)
                       ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: navArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: navRow.clicked()
        }
    }

    Column {
        anchors.fill: parent
        anchors.leftMargin: root.mini ? 8 : theme.spacingMd
        anchors.rightMargin: root.mini ? 8 : theme.spacingMd
        anchors.topMargin: root.mini ? 8 : theme.spacingMd
        anchors.bottomMargin: root.mini ? 8 : theme.spacingMd
        spacing: theme.spacingMd

        // ---- Section nav -------------------------------------------
        Column {
            id: navColumn
            width: parent.width
            spacing: 2

            // Qobuz-only sections — HIDDEN entirely while offline (ADR-010).
            NavRow {
                navId: "discover"
                name: "compass"
                label: QbzBridge.tr("Discover")
                visible: !QbzBridge.offline
                onClicked: root.activeNav = "discover"
            }
            NavRow {
                navId: "library"
                name: "music-library-2"
                label: QbzBridge.tr("Library")
                visible: !QbzBridge.offline
                // Inert — the view lands in phase 3.
                onClicked: root.activeNav = "library"
            }
            NavRow {
                navId: "local"
                name: "hard-drive"
                label: QbzBridge.tr("Local Library")
                onClicked: root.activeNav = "local"
            }
            NavRow {
                navId: "myqbz"
                name: "qbz-symbolic"
                // Slint: MyQbzBrandingState.label, default "My QBZ".
                label: QbzBridge.tr("My QBZ")
                onClicked: root.activeNav = "myqbz"
            }
        }

        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ---- Playlists header toolbar (hidden in the mini state) -----
        Row {
            visible: !root.mini
            width: parent.width
            height: 22
            spacing: 4

            Text {
                width: parent.width - 4 * 26
                height: parent.height
                text: QbzBridge.tr("PLAYLISTS")
                color: theme.textMuted
                font.pixelSize: 10
                font.letterSpacing: 1
                verticalAlignment: Text.AlignVCenter
            }
            Repeater {
                // POC-NOTE: search / new-playlist / sort / collapse are
                // visual stubs (the playlist tree is out of scope).
                model: [
                    { "icon": "search", "size": 15 },
                    { "icon": "plus", "size": 16 },
                    { "icon": "ellipsis", "size": 15 },
                    { "icon": "chevron-down", "size": 15 },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: 22
                    height: 22
                    radius: 4
                    color: hdrArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: modelData.icon
                        width: modelData.size
                        height: modelData.size
                        anchors.centerIn: parent
                        tintName: hdrArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: hdrArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                    }
                }
            }
        }

        // ---- Playlist list (out of scope) — the Slint empty state ----
        Text {
            visible: !root.mini
            width: parent.width
            text: QbzBridge.tr("No playlists yet.")
            color: theme.textMuted
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }

        // Fill the rest.
        Item { width: 1; height: 0; }
    }
}
