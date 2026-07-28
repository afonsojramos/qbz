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
    // OPAQUE base (the Slint root background is opaque surface-main too —
    // invisible while the ambient field mounts over it; this IS the D4
    // no-track fallback). The translucent chrome comes from each chrome
    // piece's OWN background going surface-card @ 0.5 above the field.
    color: theme.surfaceCard

    // App-wide dynamic background (phase 14): active when the mode pref is
    // on AND a track is loaded (D4: no track -> opaque theme restored).
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
    // The bottom-most visual layer (AppShell.slint:206-242, declared FIRST
    // so every chrome surface paints above it): the ambient album-colored
    // field + the dark dim scrim that keeps chrome text legible
    // (QBZ_BG_DIM-tunable, default 0.35).
    AmbientField {
        anchors.fill: parent
        visible: root.ambientOn
        running: root.ambientOn
    }
    Rectangle {
        anchors.fill: parent
        visible: root.ambientOn
        color: "#000000"
        opacity: QbzBridge.ambientDim
    }

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
        // Square corners (phase 12: the window is opaque; any rounding is
        // the compositor's business).
    }

    NowPlayingBarSmall {
        id: npb
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: theme.npbSmallHeight
        // Square corners (see the header note above).
    }

    Sidebar {
        id: sidebar
        anchors.left: parent.left
        anchors.top: header.bottom
        anchors.bottom: npb.top
        // Animated 3-state width lives inside the component.
    }

    // Right-side panel column — Queue and/or Lyrics, stacked vertically in
    // a shared 300px column (Feishin-style, AppShell.slint:684-707). Each
    // is toggled from its bar button and closed from its own X; the column
    // is visible when either is open, animated 0 <-> 300 (160ms).
    Rectangle {
        id: queueColumn
        anchors.right: parent.right
        anchors.top: header.bottom
        anchors.bottom: npb.top
        width: (QbzBridge.queueOpen || QbzBridge.lyricsOpen) ? theme.queuePanelWidth : 0
        clip: true
        color: root.ambientOn ? theme.surfaceCardA50 : theme.surfaceCard

        Behavior on width {
            NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
        }

        QueuePanel {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: QbzBridge.lyricsOpen
                ? (QbzBridge.queueOpen ? parent.height / 2 : 0)
                : parent.height
            visible: QbzBridge.queueOpen
        }
        Rectangle {
            visible: QbzBridge.queueOpen && QbzBridge.lyricsOpen
            anchors.left: parent.left
            anchors.right: parent.right
            y: QbzBridge.lyricsOpen && QbzBridge.queueOpen ? parent.height / 2 : 0
            height: 1
            color: theme.borderSubtle
        }
        LyricsPanel {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: QbzBridge.lyricsOpen
                ? (QbzBridge.queueOpen ? parent.height / 2 - 1 : parent.height)
                : 0
            visible: QbzBridge.lyricsOpen
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
        // Frosted content panel while the ambient background is active
        // (AppShell.slint: surface-main @ 0.22 + 1px #ffffff@0.10 hairline).
        color: root.ambientOn ? theme.surfaceMainA22 : theme.surfaceMain
        border.width: root.ambientOn ? 1 : 0
        border.color: theme.frostBorder
        clip: true

        Loader {
            anchors.fill: parent
            source: QbzBridge.currentView === "home" ? "HomeView.qml"
                : QbzBridge.currentView === "library" ? "LibraryView.qml"
                : QbzBridge.currentView === "album" ? "AlbumView.qml"
                : QbzBridge.currentView === "artist" ? "ArtistView.qml"
                : QbzBridge.currentView === "settings" ? "SettingsView.qml"
                : QbzBridge.currentView === "search" ? "SearchView.qml"
                : QbzBridge.currentView === "playlist" ? "PlaylistView.qml" : ""
        }
    }

    // Search cortinilla (phase 15): the live-search dropdown overlay, LAST
    // child so it renders above every surface (Cortinilla.slint's mount).
    Cortinilla {
        anchors.fill: parent
        headerBar: header
    }

    // --- Shared text modal (phase 16) ---------------------------------------
    // The AppShell-level modal layer (the Slint AppShell modals mount at
    // window level, ADR-009): the scrim covers the WHOLE window — sidebar /
    // header / NPB included — and the panel centers on the WINDOW, not on
    // the content frame. Views reach it via `openTextModal(title, body)`.
    property bool textModalOpen: false
    property string textModalTitle: ""
    property string textModalBody: ""
    function openTextModal(title, body) {
        textModalTitle = title
        textModalBody = body
        textModalOpen = true
    }

    Rectangle {
        visible: root.textModalOpen
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: root.textModalOpen = false
        }
        Rectangle {
            anchors.centerIn: parent
            width: Math.min(root.width - 80, 560)
            height: Math.min(root.height - 120, 460)
            radius: theme.radiusMd
            color: theme.surfaceCard
            border.width: 1
            border.color: theme.borderSubtle
            MouseArea { anchors.fill: parent }
            Column {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 14
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - 28
                        text: root.textModalTitle
                        color: theme.textPrimary
                        font.pixelSize: theme.fontHeading
                        font.weight: theme.weightSemibold
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Rectangle {
                        width: 28
                        height: 28
                        color: tmCloseArea.containsMouse ? theme.surfaceHover : "transparent"
                        radius: 6
                        QbzIcon {
                            name: "x"
                            width: 18
                            height: 18
                            anchors.centerIn: parent
                            tintName: tmCloseArea.containsMouse ? "primary" : "muted"
                        }
                        MouseArea {
                            id: tmCloseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.textModalOpen = false
                        }
                    }
                }
                Flickable {
                    width: parent.width
                    height: parent.height - 42
                    clip: true
                    contentWidth: width
                    contentHeight: tmText.implicitHeight
                    Text {
                        id: tmText
                        width: parent.width
                        text: root.textModalBody
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }

    // --- Drag ghost (DragGhost.slint, phase 17) -----------------------------
    // The dark pill that follows the cursor while tracks are dragged onto
    // the sidebar: "N tracks" for a group, or title + "artist · album" for
    // one. Visual only (never blocks the pointer).
    Rectangle {
        visible: QbzBridge.dragActive
        x: QbzBridge.dragX + 10
        y: QbzBridge.dragY + 14
        width: ghostCol.width + 28
        height: ghostCol.height + 16
        radius: 8
        color: "#e01e1e28"
        border.width: 1
        border.color: "#1fffffff"
        Column {
            id: ghostCol
            anchors.centerIn: parent
            spacing: 2
            Text {
                text: QbzBridge.dragCount > 1
                    ? QbzBridge.dragCount + " " + QbzBridge.tr("tracks")
                    : QbzBridge.dragTitle
                color: "#ffffff"
                font.pixelSize: 12
                font.weight: theme.weightMedium
            }
            Text {
                visible: QbzBridge.dragCount === 1 && QbzBridge.dragSubtitle !== ""
                text: QbzBridge.dragSubtitle
                color: "#8cffffff"
                font.pixelSize: 10
            }
        }
    }
}
