// Top header bar — QML port of crates/qbz-ui/ui/shell/HeaderBar.slint.
//
// Left: the three "sacred" nav buttons (sidebar cycle, back, forward) and
// — while the sidebar is fully closed — the compact icon-only section nav
// (nav-in-sidebar is ON in this POC, so the full text tabs never mount).
// Center: the search field, absolutely centered on the window (VISUAL
// replica — the cortinilla/live-search is out of scope; POC-NOTE).
// Right: the tri-state offline status badge with its flyout (recovery
// "Sign in" wired to QbzBridge.recoveryLogin) and the app menu (user block
// + Log Out + Close; the other Slint menu items are POC-NOTE stubs).
//
// POC-NOTE: the custom window-chrome parts (drag surface, drawn
// min/max/close WindowControls) are skipped — the POC keeps NATIVE window
// decorations.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root
    // surface-card @ 0.5 while the ambient background is active (phase 14,
    // HeaderBar.slint's with-alpha(app-background-surface-alpha)).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
       

    // The host ApplicationWindow (custom chrome); null in previews.
    property var hostWindow: null

    QbzTheme { id: theme }

    // Custom-chrome drag surface (declared FIRST so every interactive
    // element above wins hit-testing): press-and-move starts the system
    // move; double-click toggles maximize. The system grab starts only
    // after a real movement so plain clicks/double-clicks still work.
    MouseArea {
        anchors.fill: parent
        // Inert under the system titlebar (the native chrome owns
        // drag/double-click) — phase 12 titlebar toggle.
        enabled: !QbzBridge.systemTitleBar
        property bool dragStarted: false
        onPressed: dragStarted = false
        onPositionChanged: {
            if (pressed && !dragStarted && root.hostWindow) {
                dragStarted = true
                root.hostWindow.startSystemMove()
            }
        }
        onDoubleClicked: {
            if (root.hostWindow) {
                root.hostWindow.visibility = root.hostWindow.visibility === Window.Maximized
                    ? Window.Windowed : Window.Maximized
            }
        }
    }

    // Slint OfflineState.badge-state: 0 hidden / 1 hard offline / 2 manual /
    // 3 logged out (wins over the others).
    readonly property int badgeState: QbzBridge.offlineSession ? 3
        : QbzBridge.offlineMode === 2 ? 2
        : QbzBridge.offlineMode === 1 ? 1 : 0

    // Circular history / collapse button (HeaderNavBtn).
    component HeaderNavBtn: Rectangle {
        property string name: ""
        property bool btnEnabled: true
        signal clicked()

        width: 28
        height: 28
        radius: 14
        opacity: btnEnabled ? 1.0 : 0.4
        color: (hnArea.containsMouse && btnEnabled) ? theme.surfaceHover : theme.surfaceElevated

        QbzIcon {
            name: parent.name
            width: 15
            height: 15
            anchors.centerIn: parent
            tintName: parent.btnEnabled ? "primary" : "muted"
        }
        MouseArea {
            id: hnArea
            anchors.fill: parent
            enabled: parent.btnEnabled
            hoverEnabled: true
            cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: parent.clicked()
        }
    }

    // Square icon button (HeaderIconButton, 36px) / compact nav (30px).
    component HeaderIconBtn: Rectangle {
        property string name: ""
        property int btnSize: 36
        property int iconSize: 19
        property bool active: false
        signal clicked()

        width: btnSize
        height: btnSize
        radius: theme.radiusSm
        color: active ? theme.surfaceElevated
             : hiArea.containsMouse ? theme.surfaceHover : "transparent"

        QbzIcon {
            name: parent.name
            width: parent.iconSize
            height: parent.iconSize
            anchors.centerIn: parent
            tintName: parent.active ? "accent"
                : hiArea.containsMouse ? "primary" : "secondary"
        }
        MouseArea {
            id: hiArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // --- Left controls ---------------------------------------------------
    Row {
        id: leftControls
        x: theme.spacingMd
        y: (root.height - height) / 2
        height: 36
        spacing: 6

        HeaderNavBtn {
            name: "panel-left"
            anchors.verticalCenter: parent.verticalCenter
            onClicked: QbzBridge.cycleSidebar()
        }
        HeaderNavBtn {
            name: "chevron-left"
            anchors.verticalCenter: parent.verticalCenter
            btnEnabled: QbzBridge.canBack
            onClicked: QbzBridge.navigateBack()
        }
        HeaderNavBtn {
            name: "chevron-right"
            anchors.verticalCenter: parent.verticalCenter
            btnEnabled: QbzBridge.canForward
            onClicked: QbzBridge.navigateForward()
        }

        // Compact section nav — only while the sidebar is fully closed
        // (Cerrado), so the sections stay reachable. POC-NOTE: these open
        // dropdown menus in Slint; here they are inert visual replicas.
        Row {
            visible: QbzBridge.sidebarState === 2
            height: parent.height
            spacing: 2

            Item { width: 6; height: 1 }
            HeaderIconBtn {
                name: "compass"
                btnSize: 30; iconSize: 16
                anchors.verticalCenter: parent.verticalCenter
                visible: !QbzBridge.offline
            }
            HeaderIconBtn {
                name: "music-library-2"
                btnSize: 30; iconSize: 16
                anchors.verticalCenter: parent.verticalCenter
                visible: !QbzBridge.offline
            }
            HeaderIconBtn {
                name: "hard-drive"
                btnSize: 30; iconSize: 16
                anchors.verticalCenter: parent.verticalCenter
            }
            HeaderIconBtn {
                name: "qbz-symbolic"
                btnSize: 30; iconSize: 16
                anchors.verticalCenter: parent.verticalCenter
            }
            // Thin separator + the playlists flyout button.
            Rectangle {
                width: 1
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                color: theme.borderSubtle
            }
            HeaderIconBtn {
                name: "list-music"
                btnSize: 30; iconSize: 16
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // --- Search (absolutely centered; LIVE — phase 15) ---------------------
    // The Slint HeaderBar search-scope: typing drives the cortinilla (220ms
    // debounce, >= 2 chars), arrows move the keyboard selection, Enter runs
    // the row / full search, Esc dismisses. The × clears + closes.
    function clearSearch() {
        searchInput.text = ""
        QbzBridge.cortinillaDismiss()
    }

    Rectangle {
        id: searchBox
        x: (root.width - width) / 2
        y: (root.height - height) / 2
        width: root.width < 960 ? 179 : 256
        height: 32
        radius: 6
        border.width: 1
        border.color: searchInput.activeFocus ? theme.accent : theme.borderSubtle
        color: theme.surfaceElevated

        QbzIcon {
            name: "search"
            width: 14
            height: 14
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            tintName: "muted"
        }
        TextInput {
            id: searchInput
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 30
            anchors.rightMargin: 8
            height: parent.height
            color: theme.textPrimary
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: text === "" && !activeFocus ? Text.AlignHCenter : Text.AlignLeft
            clip: true
            onTextEdited: {
                if (text.trim().length < 2) {
                    liveDebounce.stop()
                    QbzBridge.cortinillaDismiss()
                } else {
                    liveDebounce.restart()
                }
            }

            // 220ms live debounce (CORTINILLA_DEBOUNCE — one load per pause,
            // not one per keystroke).
            Timer {
                id: liveDebounce
                interval: 220
                repeat: false
                onTriggered: QbzBridge.searchLive(searchInput.text)
            }

            // The Enter rule (HeaderBar.slint on-enter): cortinilla open +
            // a keyboard selection -> activate the row; open + none -> full
            // search; closed -> plain submit (also Search > All).
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Down) {
                    if (QbzBridge.cortinillaOpen) {
                        QbzBridge.cortinillaMoveSelection(1)
                        event.accepted = true
                    }
                } else if (event.key === Qt.Key_Up) {
                    if (QbzBridge.cortinillaOpen) {
                        QbzBridge.cortinillaMoveSelection(-1)
                        event.accepted = true
                    }
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (QbzBridge.cortinillaOpen) {
                        if (QbzBridge.selectedIndex >= 0) {
                            root.clearSearch()
                            QbzBridge.cortinillaRowClicked(QbzBridge.selectedIndex)
                        } else {
                            root.clearSearch()
                            QbzBridge.cortinillaSearchAll()
                        }
                    } else {
                        // Capture BEFORE clearing (clearSearch wipes the input).
                        var q = searchInput.text
                        root.clearSearch()
                        QbzBridge.searchSubmit(q)
                    }
                    event.accepted = true
                } else if (event.key === Qt.Key_Escape) {
                    if (QbzBridge.cortinillaOpen) {
                        QbzBridge.cortinillaDismiss()
                        event.accepted = true
                    }
                }
            }
        }
        // Placeholder (centered when empty + unfocused, left once typing).
        Text {
            visible: searchInput.text === "" && !searchInput.activeFocus
            anchors.fill: parent
            anchors.leftMargin: 30
            text: QbzBridge.tr("Search")
            color: theme.textMuted
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
        }
        // Right-edge affordances: the Enter hint while the cortinilla is
        // open (Slint: it lives in the box, opposite the magnifier), else
        // the × clear.
        Text {
            visible: QbzBridge.cortinillaOpen
            anchors.right: parent.right
            anchors.rightMargin: 10
            height: parent.height
            text: "⏎ " + QbzBridge.tr("Enter")
            color: theme.textMuted
            font.pixelSize: 12
            verticalAlignment: Text.AlignVCenter
        }
        Rectangle {
            visible: !QbzBridge.cortinillaOpen && searchInput.text !== ""
            anchors.right: parent.right
            anchors.rightMargin: 5
            width: 22
            height: 22
            anchors.verticalCenter: parent.verticalCenter
            radius: 11
            color: clearArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: "x"
                width: 12
                height: 12
                anchors.centerIn: parent
                tintName: clearArea.containsMouse ? "primary" : "muted"
            }
            MouseArea {
                id: clearArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.clearSearch()
            }
        }
    }

    // --- Right controls: status badge + app menu --------------------------
    Row {
        id: rightControls
        // Shifts left of the drawn window controls (3x34 + 2x2 = 106px).
        x: root.width - width - theme.spacingMd + 2 - (QbzBridge.systemTitleBar ? 0 : 110)
        y: (root.height - height) / 2
        height: 36
        spacing: 4

        // Offline status badge (OfflineStatusBadge) — ghost chrome like
        // NavTab: transparent, hover -> surface-hover, radius sm.
        Rectangle {
            visible: root.badgeState !== 0
            height: 30
            width: badgeRow.implicitWidth
            anchors.verticalCenter: parent.verticalCenter
            radius: theme.radiusSm
            color: badgeArea.containsMouse ? theme.surfaceHover : "transparent"

            readonly property string stateTintName: root.badgeState === 1 ? "warning"
                : root.badgeState === 2 ? "accent" : "muted"

            Row {
                id: badgeRow
                height: parent.height
                leftPadding: 9
                rightPadding: 11
                spacing: 6
                QbzIcon {
                    name: root.badgeState === 3 ? "user" : "cloud-off"
                    width: 14
                    height: 14
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: parent.parent.stateTintName
                }
                Text {
                    height: parent.height
                    text: root.badgeState === 3 ? QbzBridge.tr("Logged out")
                        : root.badgeState === 2 ? QbzBridge.tr("Offline (manual)")
                        : QbzBridge.tr("Offline (hard)")
                    color: theme.textSecondary
                    font.pixelSize: 11
                    verticalAlignment: Text.AlignVCenter
                }
            }
            MouseArea {
                id: badgeArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: badgeFlyout.open()
            }

            // Status flyout — the former banner texts + actions.
            Popup {
                id: badgeFlyout
                x: parent.width - 320
                y: parent.height + 6
                width: 320
                padding: 14
                closePolicy: Popup.CloseOnPressOutside

                background: Rectangle {
                    color: theme.surfaceMain
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderMuted
                }
                contentItem: Column {
                    spacing: 12
                    Row {
                        spacing: 8
                        width: parent.width
                        QbzIcon {
                            name: root.badgeState === 3 ? "user" : "cloud-off"
                            width: 15
                            height: 15
                            tintName: "muted"
                        }
                        Text {
                            width: parent.width - 23
                            text: root.badgeState === 3
                                ? (QbzBridge.connectivity === 2
                                    ? QbzBridge.tr("You're signed out. Sign-in needs a connection.")
                                    : QbzBridge.connectivity === 1
                                        ? QbzBridge.tr("Connection available — sign back in to Qobuz.")
                                        : QbzBridge.tr("You're signed out — sign back in to Qobuz."))
                                : root.badgeState === 2
                                    ? QbzBridge.tr("Offline mode is enabled. Disable it in Settings to use Qobuz.")
                                    : QbzBridge.tr("No internet connection — your local library and downloads keep working.")
                            color: theme.textPrimary
                            font.pixelSize: 12
                            wrapMode: Text.WordWrap
                        }
                    }
                    // Sign in — logged-out state only; disabled only when the
                    // link is CONFIRMED down.
                    Item {
                        visible: root.badgeState === 3
                        width: parent.width
                        height: visible ? 32 : 0
                        Rectangle {
                            anchors.right: parent.right
                            width: signInText.implicitWidth + 28
                            height: 32
                            radius: theme.radiusSm
                            border.width: 1
                            border.color: theme.borderSubtle
                            opacity: QbzBridge.connectivity === 2 ? 0.4 : 1.0
                            color: signInArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                            Text {
                                id: signInText
                                anchors.centerIn: parent
                                text: QbzBridge.tr("Sign in")
                                color: theme.textSecondary
                                font.pixelSize: 13
                            }
                            MouseArea {
                                id: signInArea
                                anchors.fill: parent
                                enabled: QbzBridge.connectivity !== 2
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    QbzBridge.recoveryLogin()
                                    badgeFlyout.close()
                                }
                            }
                        }
                    }
                    // POC-NOTE: the manual-offline quick toggle (state 2)
                    // needs OfflineModeActions.set-offline — not wired; the
                    // status text above still renders.
                }
            }
        }

        HeaderIconBtn {
            name: "menu"
            anchors.verticalCenter: parent.verticalCenter
            onClicked: appMenu.open()
        }
    }

    // --- Drawn window controls (WindowControls.slint, right placement:
    // minimize · maximize · close; close gets the danger-red hover) ------
    Row {
        visible: !QbzBridge.systemTitleBar
        x: root.width - width - 8
        y: (root.height - height) / 2
        height: 26
        spacing: 2
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcMinArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: "minus"
                width: 14
                height: 14
                anchors.centerIn: parent
                tintName: "secondary"
            }
            MouseArea {
                id: wcMinArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: if (root.hostWindow) root.hostWindow.showMinimized()
            }
        }
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcMaxArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: root.hostWindow && root.hostWindow.visibility === Window.Maximized
                    ? "minimize-2" : "maximize-2"
                width: 14
                height: 14
                anchors.centerIn: parent
                tintName: "secondary"
            }
            MouseArea {
                id: wcMaxArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: {
                    if (root.hostWindow) {
                        root.hostWindow.visibility = root.hostWindow.visibility === Window.Maximized
                            ? Window.Windowed : Window.Maximized
                    }
                }
            }
        }
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcCloseArea.containsMouse ? "#e81123" : "transparent"
            QbzIcon {
                name: "x"
                width: 14
                height: 14
                anchors.centerIn: parent
                tintName: wcCloseArea.containsMouse ? "primary" : "secondary"
            }
            MouseArea {
                id: wcCloseArea
                anchors.fill: parent
                hoverEnabled: true
                // POC-NOTE: the Slint app hides-to-tray on close; the POC
                // has no tray — close quits.
                onClicked: Qt.quit()
            }
        }
    }

    // --- App menu (user block + Settings + Log Out + Close) ---------------
    // POC-NOTE: the Slint menu also carries Open Music Link / Keyboard
    // Shortcuts / Documentation / Report an Issue / What's New / About QBZ
    // — omitted here (no views/dialogs to open yet).
    Popup {
        id: appMenu
        x: root.width - 234 - theme.spacingMd
        y: theme.headerHeight - 4
        width: 234
        padding: 0
        closePolicy: Popup.CloseOnPressOutside

        background: Rectangle {
            color: theme.surfaceMain
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderMuted
        }
        contentItem: Column {
            width: parent.width
            topPadding: 6
            bottomPadding: 6

            // Signed-in user — name and subscription tier.
            Column {
                width: parent.width
                leftPadding: 14
                rightPadding: 14
                topPadding: 6
                bottomPadding: 10
                spacing: 2
                Text {
                    text: QbzBridge.sessionUserName === ""
                        ? QbzBridge.tr("Guest") : QbzBridge.sessionUserName
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightSemibold
                }
                Text {
                    text: QbzBridge.sessionSubscription === ""
                        ? QbzBridge.tr("Not signed in") : QbzBridge.sessionSubscription
                    color: theme.textMuted
                    font.pixelSize: 12
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Item { width: 1; height: 4 }

            // One menu row (MenuItem).
            component AppMenuItem: Rectangle {
                property string name: ""
                property string label: ""
                property bool checkedItem: false
                signal clicked()

                width: parent ? parent.width : 0
                height: 34
                color: miArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 18
                    spacing: 10
                    QbzIcon {
                        name: parent.parent.name
                        width: 15
                        height: 15
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "secondary"
                    }
                    Text {
                        id: miLabel
                        height: parent.height
                        text: parent.parent.label
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                    }
                    Item {
                        visible: parent.parent.checkedItem
                        width: visible ? parent.width - 15 - miLabel.implicitWidth - 14 - 2 * parent.spacing : 0
                        height: 1
                    }
                    QbzIcon {
                        visible: parent.parent.checkedItem
                        name: "check"
                        width: 14
                        height: 14
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "accent"
                    }
                }
                MouseArea {
                    id: miArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: parent.clicked()
                }
            }

            AppMenuItem {
                name: "search"
                label: QbzBridge.tr("Intelligent search")
                checkedItem: QbzBridge.intelligentSearch
                onClicked: QbzBridge.toggleIntelligentSearch()
            }
            AppMenuItem {
                name: "layout-grid"
                label: QbzBridge.tr("Ambient background")
                checkedItem: QbzBridge.ambientMode > 0
                onClicked: {
                    QbzBridge.toggleAmbientBackground()
                    // Applies LIVE (pure QML layering); the check flips now.
                }
            }
            AppMenuItem {
                name: "layout-grid"
                label: QbzBridge.tr("Use system title bar")
                checkedItem: QbzBridge.systemTitleBarPref
                onClicked: {
                    QbzBridge.toggleSystemTitleBar()
                    // Keep the menu open so the check state reads; the
                    // applied mode changes on the next launch (1:1 Slint).
                }
            }
            Text {
                width: parent.width
                leftPadding: 37
                text: QbzBridge.tr("Takes effect after restarting QBZ.")
                color: theme.textMuted
                font.pixelSize: 11
            }
            AppMenuItem {
                name: "settings-2"
                label: QbzBridge.tr("Settings")
                onClicked: {
                    appMenu.close()
                    QbzBridge.navigateTo("settings")
                }
            }
            AppMenuItem {
                name: "log-out"
                label: QbzBridge.tr("Log Out")
                onClicked: {
                    appMenu.close()
                    QbzBridge.logout()
                }
            }
            AppMenuItem {
                name: "x"
                label: QbzBridge.tr("Close")
                // POC-NOTE: the Slint app hides to tray / follows the
                // platform close behavior; the POC just quits.
                onClicked: Qt.quit()
            }
        }
    }
}
