// QBZ login screen — QML port of crates/qbz-ui/ui/login/LoginScreen.slint.
//
// Centered 720px dark card: logo, wordmark, ToS checkbox gating the
// primary button, system-browser sign-in (NO webview, NO email/password),
// phase narration, sign-in error box, offline-connectivity callout (with
// captive-portal line), session-restore error box, "Start offline" link,
// legal disclaimer.
//
// All user-visible strings go through QbzSession.tr() with the EXACT msgids
// of the Slint @tr() calls so the existing .po translations apply.
// State comes from the QbzBridge singleton (Slint's LoginState +
// OfflineState globals); actions call the bridge invokables.

import QtQuick
import com.blitzfc.qbz
import "theme"

Rectangle {
    id: root
    color: theme.surfaceMain
    // Square window corners (phase 12: opaque window; the compositor owns
    // any rounding).
    // Set by the Loader (custom chrome drag/maximize); unused here.
    property var hostWindow: null

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
                text: QbzSession.tr("QBZ", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontWordmark
                font.weight: theme.weightSemibold
                font.letterSpacing: 8
            }
            Item { width: 1; height: 2 }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: QbzSession.tr("QOBUZ™ PLAYER", QbzSession.trRev)
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
                    border.color: theme.textMuted
                    border.width: root.tosAccepted ? 0 : 1.5

                    QbzIcon {
                        anchors.centerIn: parent
                        visible: root.tosAccepted
                        name: "check"
                        width: 12
                        height: 12
                        // On the accent fill -> the on-accent selector, same
                        // as the real controls/QbzCheckbox.qml this row
                        // hand-copies (primitives/QbzCheckbox.slint:24 says
                        // accent-text, and the selector returns accent-text
                        // on 34 of the 35 palettes; it only overrides where
                        // accent-text drops under 3:1 — rose-pine-dawn,
                        // 2.56:1). The hand-copy has to track the real
                        // control or the TOS check and the Settings checks
                        // diverge on exactly that theme.
                        tintName: theme.accentGlyphTint
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.tosAccepted = !root.tosAccepted
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("I have read the", QbzSession.trRev)
                    color: theme.textSecondary
                    font.pixelSize: theme.fontBody
                }
                Text {
                    id: tosLink
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("Qobuz™ Terms of Service", QbzSession.trRev)
                    color: theme.accent
                    font.pixelSize: theme.fontBody
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzSession.openTos()
                    }
                }
            }

            Item { width: 1; height: theme.spacingLg }

            // --- Sign in ---------------------------------------------
            // Opens the user's default web browser (no embedded webview).
            Rectangle {
                id: signInButton
                width: parent.width
                height: 48
                radius: theme.radiusSm
                property bool canSignIn: root.tosAccepted && QbzSession.loginPhase === 0
                opacity: canSignIn ? 1.0 : 0.5
                color: signInArea.containsMouse && canSignIn ? theme.accentHover : theme.accent
                Behavior on color { ColorAnimation { duration: 150 } }

                Text {
                    anchors.centerIn: parent
                    text: QbzSession.tr("Sign in with your browser", QbzSession.trRev)
                    // On the accent fill (accentHover under the cursor). NOT
                    // a departure from primitives/QbzPrimaryButton.slint:36
                    // (`Theme.accent-text`), which is what the twin returns on
                    // 34 of the 35 palettes — a floor for rose-pine-dawn only
                    // (accent-text #575279 on accent #d7827e is 2.56:1).
                    color: theme.accentGlyphColor
                    font.pixelSize: theme.fontButton
                    font.weight: theme.weightSemibold
                }
                MouseArea {
                    id: signInArea
                    anchors.fill: parent
                    enabled: signInButton.canSignIn
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzSession.signInViaBrowser()
                }
            }

            Item { width: 1; height: theme.spacingMd }

            // --- Browser-flow status ---------------------------------
            // The browser may open in the background without stealing
            // focus, and the code exchange takes a few seconds — the
            // screen must narrate instead of sitting inert.
            Column {
                visible: QbzSession.loginPhase === 1
                width: parent.width
                spacing: 4
                Text {
                    width: parent.width
                    text: QbzSession.tr("Continue in your web browser — QBZ is waiting for you to finish signing in.", QbzSession.trRev)
                    color: theme.textSecondary
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Text {
                    width: parent.width
                    text: QbzSession.tr("The browser may open in the background without taking focus — check your other windows.", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }
                Item { width: 1; height: 4 }
                Item {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: cancelText.implicitWidth
                    height: cancelText.implicitHeight + 3
                    Text {
                        id: cancelText
                        text: QbzSession.tr("Cancel", QbzSession.trRev)
                        color: cancelArea.containsMouse ? theme.accent : theme.textMuted
                        font.pixelSize: theme.fontLink
                    }
                    Rectangle {
                        y: cancelText.implicitHeight + 1
                        width: cancelText.implicitWidth
                        height: 1
                        color: cancelArea.containsMouse ? theme.accent : theme.textMuted
                    }
                    MouseArea {
                        id: cancelArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzSession.cancelLogin()
                    }
                }
            }
            Text {
                visible: QbzSession.loginPhase === 2
                width: parent.width
                text: QbzSession.tr("Signing you in…", QbzSession.trRev)
                color: theme.textSecondary
                font.pixelSize: theme.fontBody
                horizontalAlignment: Text.AlignHCenter
            }
            // Sign-in failure box (phase 0 only).
            Rectangle {
                visible: QbzSession.loginPhase === 0 && QbzSession.loginError !== ""
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
                        text: QbzSession.tr("Sign-in failed", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody
                        font.weight: theme.weightMedium
                        horizontalAlignment: Text.AlignHCenter
                    }
                    Text {
                        width: parent.width
                        text: QbzSession.loginError
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
                visible: QbzSession.hasPreviousSession && QbzSession.connectivity === 2
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
                            text: QbzSession.tr("No internet connection — you can start in offline mode with your local library and downloads", QbzSession.trRev)
                            color: theme.textSecondary
                            font.pixelSize: theme.fontBody
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            visible: QbzSession.captivePortal
                            width: parent.width
                            text: QbzSession.tr("A network sign-in page may be blocking the connection", QbzSession.trRev)
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
                visible: QbzSession.restoreError !== "" && QbzSession.connectivity !== 2
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
                            text: QbzSession.tr("Could not restore your session", QbzSession.trRev)
                            color: theme.textPrimary
                            font.pixelSize: theme.fontBody
                            font.weight: theme.weightMedium
                            horizontalAlignment: Text.AlignHCenter
                        }
                        Text {
                            width: parent.width
                            text: QbzSession.restoreError
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
            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: offlineText.implicitWidth
                height: offlineText.implicitHeight + 3
                Text {
                    id: offlineText
                    text: QbzSession.tr("Start offline (no access to Qobuz™ services)", QbzSession.trRev)
                    color: offlineArea.containsMouse ? theme.accent : theme.textMuted
                    font.pixelSize: theme.fontLink
                }
                Rectangle {
                    y: offlineText.implicitHeight + 1
                    width: offlineText.implicitWidth
                    height: 1
                    color: offlineArea.containsMouse ? theme.accent : theme.textMuted
                }
                MouseArea {
                    id: offlineArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzSession.startOffline()
                }
            }

            Item { width: 1; height: theme.spacingXl }

            // --- Legal disclaimer ------------------------------------
            Text {
                width: parent.width
                text: QbzSession.tr("QBZ requires an active Qobuz™ subscription. Your credentials are sent directly to Qobuz™.", QbzSession.trRev)
                    + "\n"
                    + QbzSession.tr("QBZ can be used as an offline player without a Qobuz™ account (no access to the Qobuz™ library).", QbzSession.trRev)
                    + "\n"
                    + QbzSession.tr("This application uses the Qobuz API but is not certified by Qobuz.", QbzSession.trRev)
                    + " "
                    + QbzSession.tr("Qobuz™ is a trademark of Qobuz. QBZ is an open-source application licensed under the MIT License and is not affiliated with, endorsed by, or certified by Qobuz.", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }
        }
    }
}
