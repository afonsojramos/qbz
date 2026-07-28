// Settings > Integrations (phase 19) — the QML port of
// crates/qbz-ui/ui/settings/IntegrationsSettings.slint. Row order, group
// headers and auth-gating mirror the Slint 1:1; all state rides the single
// settingsJson document (integrations_qt.rs over the SAME scrobbler /
// discover / ui_prefs stores the Slint app uses).
//
// POC-NOTEs (named in integrations_qt.rs too):
// - No live scrobbling (scrobble::start / on_track_changed not ported) —
//   toggles + credentials persist for the Slint scrobbler / a future port.
// - Discord: the pref + live DiscordRpc flag flip; presence updates are
//   not fed (no NowListening pushes on track change).
// - The Last.fm connect flow opens the system browser (open::that) — the
//   offscreen smoke never clicks it.

import QtQuick
import com.blitzfc.qbz

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // ======================= RECOMMENDATIONS =============================
    GroupHeader { text: QbzBridge.tr("RECOMMENDATIONS") }
    SettingRow {
        label: QbzBridge.tr("Show Recommendations in Discover")
        description: QbzBridge.tr("A personalized Discover tab built from your listening. For the best results connect Last.fm and ListenBrainz below — MusicBrainz is used automatically to match releases.")
        QbzToggle {
            checked: root.doc.showRecommendations === true
            onToggled: function (v) { QbzBridge.settingsBool("show-recommendations", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // =========================== METADATA ================================
    GroupHeader { text: QbzBridge.tr("METADATA") }
    SettingRow {
        label: QbzBridge.tr("MusicBrainz")
        description: QbzBridge.tr("Enable artist relationships and enhanced metadata from MusicBrainz. No telemetry — it only matches releases to enrich artist pages and playlist song suggestions. Turn off to disable those sections.")
        QbzToggle {
            checked: root.doc.musicbrainzEnabled === true
            onToggled: function (v) { QbzBridge.settingsBool("musicbrainz", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
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
                text: QbzBridge.tr("SCROBBLERS")
                color: theme.textMuted
                font.pixelSize: 11
                font.letterSpacing: 1.5
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: QbzBridge.tr("Send your plays to Last.fm and ListenBrainz. Works for Qobuz, local, and Plex tracks.")
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
                    name: root.doc.scrobbleUiCollapsed === true ? "chevron-right" : "chevron-down"
                    width: 16
                    height: 16
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

        // ------------------------- LAST.FM ------------------------------
        GroupHeader { text: QbzBridge.tr("LAST.FM") }
        SettingRow {
            label: QbzBridge.tr("Scrobble to Last.fm")
            description: root.doc.lastfmAuthed === true
                ? QbzBridge.tr("Signed in as {}.").replace("{}", root.doc.lastfmUsername || "")
                : QbzBridge.tr("Connect your Last.fm account to enable scrobbling.")
            QbzToggle {
                checked: root.doc.lastfmEnabled === true
                onToggled: function (v) { QbzBridge.settingsBool("lastfm-enable", v) }
            }
        }
        SettingRow {
            visible: root.doc.lastfmAuthed !== true
            label: QbzBridge.tr("Connect")
            description: QbzBridge.tr("Authorize QBZ in your browser, then click Finish.")
            Row {
                spacing: 8
                SettingsButton {
                    text: root.doc.lastfmBusy === true ? QbzBridge.tr("Working...") : QbzBridge.tr("Connect Last.fm")
                    enabled: root.doc.lastfmBusy !== true
                    onClicked: QbzBridge.integrationsAction("lastfm-connect")
                }
                SettingsButton {
                    visible: (root.doc.lastfmAuthUrl || "") !== ""
                    text: QbzBridge.tr("Open authorize page")
                    onClicked: QbzBridge.integrationsAction("lastfm-open-auth-url")
                }
                SettingsButton {
                    visible: (root.doc.lastfmAuthUrl || "") !== ""
                    text: QbzBridge.tr("Finish")
                    onClicked: QbzBridge.integrationsAction("lastfm-finish")
                }
            }
        }
        SettingRow {
            visible: root.doc.lastfmAuthed === true
            label: QbzBridge.tr("Disconnect Last.fm")
            description: QbzBridge.tr("Sign out of Last.fm.")
            SettingsButton {
                danger: true
                text: QbzBridge.tr("Disconnect")
                onClicked: QbzBridge.integrationsAction("lastfm-disconnect")
            }
        }

        SettingsSpacer { }

        // ----------------------- LISTENBRAINZ ----------------------------
        GroupHeader { text: QbzBridge.tr("LISTENBRAINZ") }
        SettingRow {
            label: QbzBridge.tr("Scrobble to ListenBrainz")
            description: root.doc.listenbrainzAuthed === true
                ? QbzBridge.tr("Signed in as {}.").replace("{}", root.doc.listenbrainzUsername || "")
                : QbzBridge.tr("Paste your ListenBrainz user token to enable scrobbling.")
            QbzToggle {
                checked: root.doc.listenbrainzEnabled === true
                onToggled: function (v) { QbzBridge.settingsBool("listenbrainz-enable", v) }
            }
        }
        SettingRow {
            visible: root.doc.listenbrainzAuthed !== true
            label: QbzBridge.tr("User token")
            description: QbzBridge.tr("From listenbrainz.org/settings.")
            QbzLineEdit {
                isPassword: true
                placeholder: QbzBridge.tr("ListenBrainz token")
                onCommitted: function (v) { QbzBridge.settingsString("listenbrainz-token", v) }
            }
        }
        SettingRow {
            visible: root.doc.listenbrainzAuthed === true
            label: QbzBridge.tr("Disconnect ListenBrainz")
            description: QbzBridge.tr("Sign out of ListenBrainz.")
            SettingsButton {
                danger: true
                text: QbzBridge.tr("Disconnect")
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
    SettingsDivider { }
    SettingsSpacer { }

    // =========================== DISCORD =================================
    GroupHeader { text: QbzBridge.tr("DISCORD") }
    SettingRow {
        label: QbzBridge.tr("Discord Rich Presence")
        description: QbzBridge.tr("Show what you're listening to as your Discord status. Opt-in — Discord must be running.")
        QbzToggle {
            checked: root.doc.discordEnabled === true
            onToggled: function (v) { QbzBridge.settingsBool("discord-rpc", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Flatpak socket access")
        description: QbzBridge.tr("Flatpak install and the presence isn't showing? Grant access to Discord's IPC socket, then restart QBZ:\nflatpak override --user --filesystem=xdg-run/discord-ipc-0 com.blitzfc.qbz")
    }
}
