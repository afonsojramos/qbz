// Shell placeholder for phase 1 — proves the post-login state (session
// header, D2 recovery banner, offline-session badge, logout) without
// porting the real shell (phase 2).

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    color: theme.surfaceMain

    QbzTheme { id: theme }

    Column {
        anchors.centerIn: parent
        spacing: theme.spacingMd
        width: Math.min(parent.width - 64, 560)

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "QBZ Qt POC — shell (phase 2 pending)"
            color: theme.textPrimary
            font.pixelSize: 22
            font.weight: theme.weightSemibold
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzBridge.sessionUserName
                  + (QbzBridge.sessionSubscription !== ""
                     ? " — " + QbzBridge.sessionSubscription : "")
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: QbzBridge.offlineSession
            text: "Offline session — Qobuz™ services unavailable"
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
        }

        // D2 recovery banner: a saved session exists but the app is
        // running unauthenticated while connectivity is UP.
        Rectangle {
            visible: QbzBridge.showRecoveryBanner
            width: parent.width
            height: recoveryColumn.implicitHeight + 24
            color: theme.surfaceElevated
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderSubtle
            Column {
                id: recoveryColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 12
                spacing: 8
                Text {
                    width: parent.width
                    text: "Your session could not be restored — sign in again to regain access to Qobuz™."
                    color: theme.textSecondary
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: recoveryButtonText.implicitWidth + 32
                    height: 36
                    radius: theme.radiusSm
                    color: recoveryArea.containsMouse ? "#5a95f5" : theme.accent
                    Text {
                        id: recoveryButtonText
                        anchors.centerIn: parent
                        text: "Sign in again"
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody
                        font.weight: theme.weightMedium
                    }
                    MouseArea {
                        id: recoveryArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.recoveryLogin()
                    }
                }
            }
        }

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: logoutText.implicitWidth + 32
            height: 36
            radius: theme.radiusSm
            color: theme.surfaceElevated
            border.width: 1
            border.color: theme.borderSubtle
            Text {
                id: logoutText
                anchors.centerIn: parent
                text: "Log out"
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzBridge.logout()
            }
        }
    }
}
