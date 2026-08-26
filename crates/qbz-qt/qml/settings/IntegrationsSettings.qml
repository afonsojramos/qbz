// Settings > Integrations — the QML port of
// crates/qbz-ui/ui/settings/IntegrationsSettings.slint. Row order, group
// headers and auth-gating mirror the Slint 1:1; all state rides the single
// settingsJson document (integrations_qt.rs over the SAME scrobbler /
// discover / ui_prefs / offline-queue stores the Slint app uses).
//
// Every integration here is STRICTLY OPT-IN: nothing connects, fires or
// queues until the user signs the service in and its toggles are on.
//
// NOTE: the Last.fm connect flow opens the system browser (open::that) — the
// offscreen smoke never clicks it.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // The Slint mirrors the in-progress token in
    // ScrobbleState.listenbrainz-token-input so its "Connect ListenBrainz"
    // button can submit it. QbzLineEdit only publishes the COMMITTED value
    // (it commits on Enter / focus loss, and a SettingsButton click does not
    // steal focus from the TextInput), so read the live value off its inner
    // input — the one child that has an echoMode.
    function lbTokenText() {
        for (var i = 0; i < lbTokenField.children.length; ++i) {
            var c = lbTokenField.children[i]
            if (c && c.echoMode !== undefined)
                return c.text
        }
        return ""
    }

    // ======================= RECOMMENDATIONS =============================
    GroupHeader { text: QbzSession.tr("RECOMMENDATIONS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Show Recommendations in Discover", QbzSession.trRev)
        description: QbzSession.tr("A personalized Discover tab built from your listening. For the best results connect Last.fm and ListenBrainz below — MusicBrainz is used automatically to match releases.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.showRecommendations === true
            onToggled: function (v) { QbzBridge.settingsBool("show-recommendations", v) }
        }
    }

    SettingsSpacer { }

    // =========================== METADATA ================================
    GroupHeader { text: QbzSession.tr("METADATA", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("MusicBrainz", QbzSession.trRev)
        description: QbzSession.tr("Enable artist relationships and enhanced metadata from MusicBrainz. No telemetry — it only matches releases to enrich artist pages and playlist song suggestions. Turn off to disable those sections.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.musicbrainzEnabled === true
            onToggled: function (v) { QbzBridge.settingsBool("musicbrainz", v) }
        }
    }

    SettingsSpacer { }

    // ========================== SCROBBLERS ===============================
    // Master header row (toggle + collapse chevron), then the body gated
    // on enabled && !collapsed (IntegrationsSettings.slint:113-169).
    Item {
        width: parent.width
        height: 64
        Column {
            anchors.left: parent.left
            anchors.right: scrobbleControl.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            spacing: 3
            Text {
                width: parent.width
                text: QbzSession.tr("SCROBBLERS", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 11
                font.letterSpacing: 1.5
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: QbzSession.tr("Send your plays to Last.fm and ListenBrainz. Works for Qobuz, local, and Plex tracks.", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }
        }
        Row {
            id: scrobbleControl
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            // Collapse chevron (only when the master toggle is on).
            Rectangle {
                visible: root.doc.scrobbleEnabled === true
                width: 28
                height: 28
                radius: theme.radiusSm
                color: colArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    // 28px hit-box / 18px glyph (IntegrationsSettings.slint:129).
                    name: root.doc.scrobbleUiCollapsed === true ? "chevron-right" : "chevron-down"
                    width: 18
                    height: 18
                    tintName: "secondary"
                }
                MouseArea {
                    id: colArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.settingsBool("scrobble-collapse", root.doc.scrobbleUiCollapsed !== true)
                }
            }
            QbzToggle {
                anchors.verticalCenter: parent.verticalCenter
                checked: root.doc.scrobbleEnabled === true
                onToggled: function (v) { QbzBridge.settingsBool("scrobble-enable", v) }
            }
        }
    }

    Column {
        visible: root.doc.scrobbleEnabled === true && root.doc.scrobbleUiCollapsed !== true
        width: parent.width
        spacing: 4

        SettingRow {
            label: QbzSession.tr("Scrobble while logged out", QbzSession.trRev)
            description: QbzSession.tr("Continue sending plays to connected scrobblers without a Qobuz session.", QbzSession.trRev)
            QbzToggle {
                checked: root.doc.allowLoggedOutScrobbling !== false
                onToggled: function (v) {
                    QbzBridge.settingsBool("scrobble-logged-out", v)
                }
            }
        }

        SettingsSpacer { }

        // ------------------------- LAST.FM ------------------------------
        GroupHeader { text: QbzSession.tr("LAST.FM", QbzSession.trRev) }
        SettingRow {
            label: QbzSession.tr("Scrobble to Last.fm", QbzSession.trRev)
            description: root.doc.lastfmAuthed === true
                ? QbzSession.tr("Signed in as {}.", QbzSession.trRev).replace("{}", root.doc.lastfmUsername || "")
                : QbzSession.tr("Connect your Last.fm account to enable scrobbling.", QbzSession.trRev)
            QbzToggle {
                checked: root.doc.lastfmEnabled === true
                onToggled: function (v) { QbzBridge.settingsBool("lastfm-enable", v) }
            }
        }
        SettingRow {
            visible: root.doc.lastfmAuthed !== true
            label: QbzSession.tr("Connect", QbzSession.trRev)
            description: QbzSession.tr("Authorize QBZ in your browser, then click Finish.", QbzSession.trRev)
            Row {
                spacing: 8
                SettingsButton {
                    text: root.doc.lastfmBusy === true ? QbzSession.tr("Working...", QbzSession.trRev) : QbzSession.tr("Connect Last.fm", QbzSession.trRev)
                    enabled: root.doc.lastfmBusy !== true
                    onClicked: QbzBridge.integrationsAction("lastfm-connect")
                }
                SettingsButton {
                    visible: (root.doc.lastfmAuthUrl || "") !== ""
                    text: QbzSession.tr("Open authorize page", QbzSession.trRev)
                    onClicked: QbzBridge.integrationsAction("lastfm-open-auth-url")
                }
                SettingsButton {
                    visible: (root.doc.lastfmAuthUrl || "") !== ""
                    text: root.doc.lastfmBusy === true
                        ? QbzSession.tr("Working...", QbzSession.trRev)
                        : QbzSession.tr("Finish", QbzSession.trRev)
                    enabled: root.doc.lastfmBusy !== true
                    onClicked: QbzBridge.integrationsAction("lastfm-finish")
                }
            }
        }
        SettingRow {
            visible: root.doc.lastfmAuthed === true
            label: QbzSession.tr("Disconnect Last.fm", QbzSession.trRev)
            description: QbzSession.tr("Sign out of Last.fm.", QbzSession.trRev)
            SettingsButton {
                danger: true
                text: QbzSession.tr("Disconnect", QbzSession.trRev)
                onClicked: QbzBridge.integrationsAction("lastfm-disconnect")
            }
        }

        SettingsSpacer { }

        // ----------------------- LISTENBRAINZ ----------------------------
        GroupHeader { text: QbzSession.tr("LISTENBRAINZ", QbzSession.trRev) }
        SettingRow {
            label: QbzSession.tr("Scrobble to ListenBrainz", QbzSession.trRev)
            description: root.doc.listenbrainzAuthed === true
                ? QbzSession.tr("Signed in as {}.", QbzSession.trRev).replace("{}", root.doc.listenbrainzUsername || "")
                : QbzSession.tr("Paste your ListenBrainz user token to enable scrobbling.", QbzSession.trRev)
            QbzToggle {
                checked: root.doc.listenbrainzEnabled === true
                onToggled: function (v) { QbzBridge.settingsBool("listenbrainz-enable", v) }
            }
        }
        SettingRow {
            visible: root.doc.listenbrainzAuthed !== true
            label: QbzSession.tr("User token", QbzSession.trRev)
            description: QbzSession.tr("From listenbrainz.org/settings.", QbzSession.trRev)
            QbzLineEdit {
                id: lbTokenField
                isPassword: true
                enabled: root.doc.listenbrainzBusy !== true
                placeholder: QbzSession.tr("ListenBrainz token", QbzSession.trRev)
                // Enter (and focus loss) validates, like the Slint `accepted`.
                onCommitted: function (v) { QbzBridge.settingsString("listenbrainz-token", v) }
                // The row hides once the sign-in lands; drop the typed token
                // with it (Slint resets listenbrainz-token-input on success).
                onVisibleChanged: if (!visible) text = ""
            }
        }
        SettingRow {
            visible: root.doc.listenbrainzAuthed !== true
            label: QbzSession.tr("Save token", QbzSession.trRev)
            description: QbzSession.tr("Validates the token against ListenBrainz.", QbzSession.trRev)
            SettingsButton {
                text: root.doc.listenbrainzBusy === true
                    ? QbzSession.tr("Validating...", QbzSession.trRev)
                    : QbzSession.tr("Connect ListenBrainz", QbzSession.trRev)
                enabled: root.doc.listenbrainzBusy !== true
                onClicked: QbzBridge.settingsString("listenbrainz-token", root.lbTokenText())
            }
        }
        SettingRow {
            visible: root.doc.listenbrainzAuthed === true
            label: QbzSession.tr("Disconnect ListenBrainz", QbzSession.trRev)
            description: QbzSession.tr("Sign out of ListenBrainz.", QbzSession.trRev)
            SettingsButton {
                danger: true
                text: QbzSession.tr("Disconnect", QbzSession.trRev)
                onClicked: QbzBridge.integrationsAction("listenbrainz-disconnect")
            }
        }

        // Status line (scrobble.rs set_status; kind 1 info / 2 ok / 3 error).
        Text {
            visible: (root.doc.integrationsStatusText || "") !== ""
            width: parent.width
            text: root.doc.integrationsStatusText || ""
            color: root.doc.integrationsStatusKind === 2 ? theme.success
                : root.doc.integrationsStatusKind === 3 ? theme.danger
                : theme.textMuted
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }
    }

    SettingsSpacer { }

    // =========================== DISCORD =================================
    GroupHeader { text: QbzSession.tr("DISCORD", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Discord Rich Presence", QbzSession.trRev)
        description: QbzSession.tr("Show what you're listening to as your Discord status. Opt-in — Discord must be running.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.discordEnabled === true
            onToggled: function (v) { QbzBridge.settingsBool("discord-rpc", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Flatpak socket access", QbzSession.trRev)
        description: QbzSession.tr("Flatpak install and the presence isn't showing? Grant access to Discord's IPC socket, then restart QBZ:\nflatpak override --user --filesystem=xdg-run/discord-ipc-0 com.blitzfc.qbz", QbzSession.trRev)
    }
}
