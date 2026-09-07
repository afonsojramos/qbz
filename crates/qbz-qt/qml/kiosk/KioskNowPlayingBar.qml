// Touch transport: shared bridge state/actions, a single 64px footprint.
// Detailed queue, seek and secondary actions live in the Now Playing route.
import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"
import "../shell"

Rectangle {
    id: root
    readonly property int barHeight: 64
    property Item tooltip: null
    implicitHeight: barHeight
    color: theme.surfaceCard
    QbzTheme { id: theme }

    component TouchButton: Rectangle {
        id: button
        property string name: ""
        property string label: ""
        property bool available: true
        property bool active: false
        signal clicked()
        width: root.barHeight
        height: root.barHeight
        color: active ? theme.surfaceElevated : "transparent"
        border.width: activeFocus ? 2 : 0
        border.color: theme.accent
        opacity: available ? 1 : 0.35
        activeFocusOnTab: available && visible
        Accessible.role: Accessible.Button
        Accessible.name: label
        Accessible.onPressAction: if (available) clicked()
        QbzIcon {
            anchors.centerIn: parent
            width: 28; height: 28
            name: button.name
            tintName: button.active ? "accent" : "textPrimary"
        }
        Keys.onPressed: function (event) {
            if (available && !event.isAutoRepeat && (event.key === Qt.Key_Space
                    || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
                clicked()
                event.accepted = true
            }
        }
        MouseArea {
            anchors.fill: parent
            enabled: button.available
            onPressed: button.forceActiveFocus()
            onClicked: button.clicked()
        }
    }

    Item {
        id: track
        anchors.left: parent.left
        anchors.right: transport.left
        height: parent.height
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: QbzSession.tr("Now Playing", QbzSession.trRev)
        Accessible.onPressAction: QbzShell.navigateTo("nowplaying")
        KioskArtwork {
            id: artwork
            x: 8; anchors.verticalCenter: parent.verticalCenter
            width: 48; height: 48
            source: QbzPlayer.npArtworkPath
        }
        Column {
            anchors.left: artwork.right
            anchors.leftMargin: 12
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                width: parent.width
                text: QbzPlayer.npHasTrack ? QbzPlayer.npTitle : QbzSession.tr("Now Playing", QbzSession.trRev)
                color: theme.textPrimary; font.pixelSize: 16
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: QbzPlayer.npLoading ? QbzSession.tr("Loading…", QbzSession.trRev) : QbzPlayer.npArtist
                color: theme.textSecondary; font.pixelSize: 14
                elide: Text.ElideRight
            }
        }
        Rectangle { anchors.fill: parent; color: "transparent"; border.width: track.activeFocus ? 2 : 0; border.color: theme.accent }
        MouseArea { anchors.fill: parent; onClicked: QbzShell.navigateTo("nowplaying") }
        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                QbzShell.navigateTo("nowplaying")
                event.accepted = true
            }
        }
    }
    Row {
        id: transport
        anchors.right: parent.right
        height: parent.height
        TouchButton {
            visible: root.width >= 1024
            name: "shuffle"; label: QbzSession.tr("Shuffle", QbzSession.trRev)
            available: QbzPlayer.npHasTrack; active: QbzPlayer.npShuffle
            onClicked: QbzPlayer.toggleShuffle()
        }
        TouchButton {
            name: "skip-back"; label: QbzSession.tr("Previous", QbzSession.trRev)
            available: QbzPlayer.npHasTrack
            onClicked: QbzPlayer.previous()
        }
        TouchButton {
            name: QbzPlayer.npPlaying ? "pause" : "play-fill"
            label: QbzPlayer.npPlaying ? QbzSession.tr("Pause", QbzSession.trRev) : QbzSession.tr("Play", QbzSession.trRev)
            active: true
            available: QbzPlayer.npHasTrack || QbzQueue.hasPlayTarget
            onClicked: QbzPlayer.togglePlay()
        }
        TouchButton {
            name: "skip-forward"; label: QbzSession.tr("Next", QbzSession.trRev)
            available: QbzPlayer.npHasTrack
            onClicked: QbzPlayer.next()
        }
        TouchButton {
            visible: root.width >= 1024
            name: QbzPlayer.npRepeatMode === 2 ? "repeat-1" : "repeat"
            label: QbzSession.tr("Repeat", QbzSession.trRev)
            available: QbzPlayer.npHasTrack; active: QbzPlayer.npRepeatMode > 0
            onClicked: QbzPlayer.cycleRepeat()
        }
        TouchButton {
            name: "list-music"; label: QbzSession.tr("Now Playing", QbzSession.trRev)
            onClicked: QbzShell.navigateTo("nowplaying")
        }
        TouchButton {
            name: "maximize-2"; label: QbzSession.tr("Immersive", QbzSession.trRev)
            onClicked: QbzImmersive.open = true
        }
        TouchButton {
            id: moreButton
            name: "ellipsis"; label: QbzSession.tr("More options", QbzSession.trRev)
            onClicked: moreMenu.open()
        }
    }
    Popup {
        id: moreMenu
        parent: Overlay.overlay
        width: Math.min(380, parent ? parent.width - 24 : 380)
        height: menuColumn.height + 24
        x: parent ? parent.width - width - 12 : 0
        y: parent ? Math.max(12, parent.height - root.barHeight - height - 8) : 0
        padding: 12
        closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape
        background: Rectangle { color: theme.surfaceCard; radius: 8; border.color: theme.borderSubtle; border.width: 1 }
        Column {
            id: menuColumn
            width: parent.width
            spacing: 8
            Row {
                spacing: 4
                TouchButton {
                    name: "shuffle"; label: QbzSession.tr("Shuffle", QbzSession.trRev)
                    available: QbzPlayer.npHasTrack; active: QbzPlayer.npShuffle
                    onClicked: QbzPlayer.toggleShuffle()
                }
                TouchButton {
                    name: QbzPlayer.npRepeatMode === 2 ? "repeat-1" : "repeat"
                    label: QbzSession.tr("Repeat", QbzSession.trRev)
                    available: QbzPlayer.npHasTrack; active: QbzPlayer.npRepeatMode > 0
                    onClicked: QbzPlayer.cycleRepeat()
                }
                TouchButton {
                    name: QbzPlayer.npMuted ? "volume-x" : "volume-2"
                    label: QbzSession.tr("Mute", QbzSession.trRev)
                    available: volumeControl.enabled
                    active: QbzPlayer.npMuted
                    onClicked: QbzPlayer.toggleMute()
                }
                TouchButton {
                    name: "settings-2"; label: QbzSession.tr("Settings", QbzSession.trRev)
                    onClicked: { moreMenu.close(); QbzShell.navigateTo("settings") }
                }
            }
            QbzSlider {
                id: volumeControl
                kioskHost: true
                width: parent.width
                minimum: 0; maximum: 1000
                enabled: !((QbzPlayer.npVolumeLocked && !QbzPlayer.npIsRemote) || QbzPlayer.npRemoteVolumeLocked)
                value: Math.round(QbzPlayer.npVolume * 1000)
                onChanged: function (v) { QbzPlayer.setVolume(v / 1000.0) }
                onReleased: function (v) { QbzPlayer.persistVolume(v / 1000.0) }
            }
            SettingsButton {
                width: parent.width; kioskHost: true
                text: QbzSession.tr("Qobuz Connect", QbzSession.trRev)
                onClicked: { moreMenu.close(); connectMenu.openAboveRight(moreButton) }
            }
            SettingsButton {
                width: parent.width; kioskHost: true
                text: QbzSession.tr("Cast", QbzSession.trRev)
                onClicked: { moreMenu.close(); QbzCast.openPicker() }
            }
            SettingsButton {
                width: parent.width; kioskHost: true
                text: QbzSession.tr("Desktop mode", QbzSession.trRev)
                onClicked: { moreMenu.close(); QbzSession.toggleProfile() }
            }
        }
    }
    QconnectFlyout { id: connectMenu }
    CastPicker { }
    Rectangle {
        width: parent.width; height: 2
        color: theme.surfaceElevated
        Rectangle { width: parent.width * Math.max(0, Math.min(1, QbzPlayer.npProgress)); height: 2; color: theme.accent }
    }
}
