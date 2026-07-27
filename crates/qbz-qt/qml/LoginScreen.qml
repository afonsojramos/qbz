// QBZ login screen — QML port of crates/qbz-ui/ui/login/LoginScreen.slint.
//
// Centered 720px dark card: logo, wordmark, ToS checkbox gating the
// primary button, system-browser sign-in (NO webview, NO email/password),
// phase narration, sign-in error box, offline-connectivity callout (with
// captive-portal line), session-restore error box, "Start offline" link,
// legal disclaimer.
//
// All user-visible strings go through QbzBridge.tr() with the EXACT msgids
// of the Slint @tr() calls so the existing .po translations apply.
// State comes from the QbzBridge singleton (Slint's LoginState +
// OfflineState globals); actions call the bridge invokables.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    color: theme.surfaceMain

    // Slint: in-out property tos-accepted: true.
    property bool tosAccepted: true

    QbzTheme { id: theme }

    // Faked drop shadow (blur 32, offset-y 8, #00000066): a translucent
    // black rounded rect behind the card. Qt5Compat DropShadow is not
    // assumed to be installed on the target.
    Rectangle {
        anchors.horizontalCenter: card.horizontalCenter
        y: card.y + 8
        width: card.width
        height: card.height
        radius: theme.radiusLg
        color: theme.cardShadow
        opacity: 0.5
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: 720
        height: cardColumn.implicitHeight + 2 * theme.cardPadding
        color: theme.surfaceCard
        radius: theme.radiusLg

        Column {
            id: cardColumn
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: theme.cardPadding
            spacing: 0

            // --- Brand ------------------------------------------------
            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                source: "assets/qbz-logo.png"
                width: 140
                height: 140
                fillMode: Image.PreserveAspectFit
            }
            Item { width: 1; height: theme.spacingSm }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: QbzBridge.tr("QBZ")
                color: theme.textPrimary
                font.pixelSize: theme.fontWordmark
                font.weight: theme.weightSemibold
                font.letterSpacing: 8
            }
            Item { width: 1; height: 2 }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: QbzBridge.tr("QOBUZ™ PLAYER")
                color: theme.textMuted
                font.pixelSize: theme.fontSubtitle
                font.letterSpacing: 4
            }

            Item { width: 1; height: theme.spacingXl }

            // --- Terms of Service row --------------------------------
            Row {
                spacing: theme.spacingSm

                // Minimal dark checkbox (Slint QbzCheckbox equivalent).
                Rectangle {
                    id: tosCheckbox
                    width: 18
                    height: 18
                    anchors.verticalCenter: parent.verticalCenter
                    radius: 4
                    color: root.tosAccepted ? theme.accent : "transparent"
                    border.color: root.tosAccepted ? theme.accent : theme.textMuted
                    border.width: 1

                    Text {
                        anchors.centerIn: parent
                        visible: root.tosAccepted
                        text: "✓"
                        color: theme.textPrimary
                        font.pixelSize: 13
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.tosAccepted = !root.tosAccepted
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzBridge.tr("I have read the")
                    color: theme.textSecondary
                    font.pixelSize: theme.fontBody
                }
                Text {
                    id: tosLink
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzBridge.tr("Qobuz™ Terms of Service")
                    color: theme.accent
                    font.pixelSize: theme.fontBody
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.openTos()
                    }
                }
            }

            Item { width: 1; height: theme.spacingLg }

            // --- Sign in ---------------------------------------------
            // Opens the user's default web browser (no embedded webview).
            Rectangle {
                id: signInButton
                width: parent.width
                height: 44
                radius: theme.radiusSm
                property bool canSignIn: root.tosAccepted && QbzBridge.loginPhase === 0
                color: canSignIn ? (signInArea.containsMouse ? "#5a95f5" : theme.accent)
                                 : theme.surfaceElevated

                Text {
                    anchors.centerIn: parent
                    text: QbzBridge.tr("Sign in with your browser")
                    color: signInButton.canSignIn ? theme.textPrimary : theme.textMuted
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightMedium
                }
                MouseArea {
                    id: signInArea
                    anchors.fill: parent
                    enabled: signInButton.canSignIn
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.signInViaBrowser()
                }
            }

            Item { width: 1; height: theme.spacingMd }

            // --- Browser-flow status ---------------------------------
            // The browser may open in the background without stealing
            // focus, and the code exchange takes a few seconds — the
            // screen must narrate instead of sitting inert.
            Column {
                visible: QbzBridge.loginPhase === 1
                width: parent.width
                spacing: 4
                Text {
                    width: parent.width
                    text: QbzBridge.tr("Continue in your web browser — QBZ is waiting for you to finish signing in.")
                    color: theme.textSecondary
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Text {
                    width: parent.width
                    text: QbzBridge.tr("The browser may open in the background without taking focus — check your other windows.")
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Item { width: 1; height: 4 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: QbzBridge.tr("Cancel")
                    color: theme.accent
                    font.pixelSize: theme.fontBody
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.cancelLogin()
                    }
                }
            }
            Text {
                visible: QbzBridge.loginPhase === 2
                width: parent.width
                text: QbzBridge.tr("Signing you in…")
                color: theme.textSecondary
                font.pixelSize: theme.fontBody
                horizontalAlignment: Text.AlignHCenter
            }
            // Sign-in failure box (phase 0 only).
            Rectangle {
                visible: QbzBridge.loginPhase === 0 && QbzBridge.loginError !== ""
                width: parent.width
                height: signInErrorColumn.implicitHeight + 24
                color: theme.surfaceElevated
                radius: theme.radiusSm
                border.width: 1
                border.color: theme.borderSubtle
                Column {
                    id: signInErrorColumn
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: 12
                    spacing: 4
                    Text {
                        width: parent.width
                        text: QbzBridge.tr("Sign-in failed")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody
                        font.weight: theme.weightMedium
                        horizontalAlignment: Text.AlignHCenter
                    }
                    Text {
                        width: parent.width
                        text: QbzBridge.loginError
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                    }
                }
            }
            Item { width: 1; height: 10 }

            // --- Offline-boot callout (spec §4.1) --------------------
            // No connectivity: point the user at the Start-offline link
            // right below. Gated on has-previous-session too.
            Column {
                visible: QbzBridge.hasPreviousSession && QbzBridge.connectivity === 2
                width: parent.width
                spacing: 2
                Rectangle {
                    width: parent.width
                    height: offlineColumn.implicitHeight + 24
                    color: theme.surfaceElevated
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderSubtle
                    Column {
                        id: offlineColumn
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 12
                        spacing: 4
                        Text {
                            width: parent.width
                            text: QbzBridge.tr("No internet connection — you can start in offline mode with your local library and downloads")
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            visible: QbzBridge.captivePortal
                            width: parent.width
                            text: QbzBridge.tr("A network sign-in page may be blocking the connection")
                            color: theme.textMuted
                            font.pixelSize: theme.fontLegal
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                    }
                }
                // Caret pointing at the Start-offline link below.
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "▼"
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
            }

            // Init-error variant of the same box (session restore failed
            // for a non-connectivity reason). Hidden while the
            // connectivity box shows — never two boxes at once.
            Column {
                visible: QbzBridge.restoreError !== "" && QbzBridge.connectivity !== 2
                width: parent.width
                spacing: 0
                Rectangle {
                    width: parent.width
                    height: restoreColumn.implicitHeight + 24
                    color: theme.surfaceElevated
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderSubtle
                    Column {
                        id: restoreColumn
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 12
                        spacing: 4
                        Text {
                            width: parent.width
                            text: QbzBridge.tr("Could not restore your session")
                            color: theme.textPrimary
                            font.pixelSize: theme.fontBody
                            font.weight: theme.weightMedium
                            horizontalAlignment: Text.AlignHCenter
                        }
                        Text {
                            width: parent.width
                            text: QbzBridge.restoreError
                            color: theme.textMuted
                            font.pixelSize: theme.fontLegal
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                    }
                }
                Item { width: 1; height: theme.spacingSm }
            }

            // Always visible (#553): without a previous session this
            // opens the GUEST profile (user 0) — Local Library only,
            // adopted by the account on the first real login.
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: QbzBridge.tr("Start offline (no access to Qobuz™ services)")
                color: theme.accent
                font.pixelSize: theme.fontBody
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.startOffline()
                }
            }

            Item { width: 1; height: theme.spacingXl }

            // --- Legal disclaimer ------------------------------------
            Text {
                width: parent.width
                text: QbzBridge.tr("QBZ requires an active Qobuz™ subscription. Your credentials are sent directly to Qobuz™.")
                    + "\n"
                    + QbzBridge.tr("QBZ can be used as an offline player without a Qobuz™ account (no access to the Qobuz™ library).")
                    + "\n"
                    + QbzBridge.tr("This application uses the Qobuz API but is not certified by Qobuz.")
                    + " "
                    + QbzBridge.tr("Qobuz™ is a trademark of Qobuz. QBZ is an open-source application licensed under the MIT License and is not affiliated with, endorsed by, or certified by Qobuz.")
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }
        }
    }
}
