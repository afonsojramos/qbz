// Settings > Import / Export — everything that moves data in or out of this
// QBZ install (2026-09-02):
//
//   SETTINGS PORTABILITY  the app's own settings bundle (moved here from
//                         Developer; the include-auth gate travels with it)
//   BLACKLIST             the portable blacklist, a JSON one user can hand to
//                         another QBZ user; import is additive
//   ACCOUNT MIGRATION     favorites + playlists from one Qobuz account into
//                         another, plus the local profile (next commits)
//
// All state comes from the settings document (`doc.importExport`); the rows
// call `QbzBridge.settingsString(...)` and Rust republishes the status lines.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var ie: doc.importExport || ({})

    QbzTheme { id: theme }

    spacing: 4

    // ====================== SETTINGS PORTABILITY =========================
    GroupHeader { text: QbzSession.tr("SETTINGS PORTABILITY", QbzSession.trRev) }
    Text {
        width: parent.width
        text: QbzSession.tr("This bundle holds the app's own settings: audio, playback, appearance, integrations. It does not contain your Qobuz favorites, playlists, blacklist or purchases.", QbzSession.trRev)
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }
    // The `--include-auth` gate. DELIBERATELY NOT PERSISTED: the reference's
    // default-OFF is re-asserted every time its modal opens, so the user
    // re-decides per export. `includeAuth` is session state and resets
    // whenever the panel is left.
    property bool includeAuth: false
    onVisibleChanged: if (!visible) root.includeAuth = false

    SettingRow {
        label: QbzSession.tr("Include sign-in credentials", QbzSession.trRev)
        description: QbzSession.tr("The bundle will contain your tokens. Anyone who opens the file can sign in as you.", QbzSession.trRev)
        QbzToggle {
            checked: root.includeAuth
            onToggled: function (v) { root.includeAuth = v }
        }
    }
    SettingRow {
        label: QbzSession.tr("Export settings…", QbzSession.trRev)
        description: QbzSession.tr("Save a portable bundle of your settings to move to another machine or the qbzd daemon.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Export…", QbzSession.trRev)
            onClicked: QbzBridge.settingsString(
                "export-settings", root.includeAuth ? "with-auth" : "")
        }
    }
    Text {
        visible: (root.ie.settingsStatus || "") !== ""
        width: parent.width
        text: root.ie.settingsStatus || ""
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }

    SettingsSpacer { }

    // ============================ BLACKLIST ==============================
    GroupHeader { text: QbzSession.tr("BLACKLIST", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Export blacklist…", QbzSession.trRev)
        description: QbzSession.tr("Save your blocked artists and albums as a file you can hand to another QBZ user.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Export…", QbzSession.trRev)
            enabled: ((root.ie.blacklistArtists || 0) + (root.ie.blacklistAlbums || 0)) > 0
            onClicked: QbzBridge.settingsString("export-blacklist", "")
        }
    }
    SettingRow {
        label: QbzSession.tr("Import blacklist…", QbzSession.trRev)
        description: QbzSession.tr("Add blocked artists and albums from a file. Nothing already on your blacklist is removed.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Import…", QbzSession.trRev)
            onClicked: QbzBridge.settingsString("import-blacklist", "")
        }
    }
    Text {
        visible: (root.ie.blacklistStatus || "") !== ""
        width: parent.width
        text: root.ie.blacklistStatus || ""
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }

    SettingsSpacer { }

    // ======================== ACCOUNT MIGRATION ==========================
    GroupHeader { text: QbzSession.tr("ACCOUNT MIGRATION", QbzSession.trRev) }
    Text {
        width: parent.width
        text: QbzSession.tr("Moves favorites, playlists and followed playlists from one Qobuz account into another, adding only what is missing and deleting nothing on either side. It does not move subscriptions or purchases: those stay with the Qobuz account that holds them.", QbzSession.trRev)
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }
    SettingRow {
        label: QbzSession.tr("Create migration snapshot", QbzSession.trRev)
        description: QbzSession.tr("Sign in with the account you are moving FROM, then save its favorites and playlists to a file in its QBZ profile.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Create…", QbzSession.trRev)
            enabled: !root.ie.migrationBusy
            onClicked: QbzBridge.settingsString("account-snapshot", "")
        }
    }
    Text {
        visible: (root.ie.snapshotStatus || "") !== ""
        width: parent.width
        text: root.ie.snapshotStatus || ""
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }
    SettingRow {
        label: QbzSession.tr("Migrate into this account", QbzSession.trRev)
        description: (root.ie.snapshots || []).length > 0
            ? QbzSession.tr("Sign in with the account you are moving TO, then pick a snapshot below. Safe to repeat: a second run adds nothing.", QbzSession.trRev)
            : QbzSession.tr("No snapshots yet. Sign in with the old account and create one first.", QbzSession.trRev)
    }
    // What the local half carries besides library, playlist folders, pinned
    // items, blacklist and preferences. Asked every time, all on by
    // default (owner decision 2026-09-02); session state, not persisted.
    property bool copyLocalProfile: true
    property bool copyMediaServers: true
    property bool copyScrobblers: true
    property bool copyListeningHistory: true
    SettingRow {
        label: QbzSession.tr("Also copy the local profile", QbzSession.trRev)
        description: QbzSession.tr("Library folders, playlist folders and order, pinned items, blacklist and preferences of the old account's QBZ profile on this computer.", QbzSession.trRev)
        QbzToggle {
            checked: root.copyLocalProfile
            onToggled: function (v) { root.copyLocalProfile = v }
        }
    }
    SettingRow {
        rowEnabled: root.copyLocalProfile
        label: QbzSession.tr("Media server connections", QbzSession.trRev)
        description: QbzSession.tr("Plex, Jellyfin and Subsonic settings, including their credentials.", QbzSession.trRev)
        QbzToggle {
            checked: root.copyMediaServers
            onToggled: function (v) { root.copyMediaServers = v }
        }
    }
    SettingRow {
        rowEnabled: root.copyLocalProfile
        label: QbzSession.tr("Scrobbler accounts", QbzSession.trRev)
        description: QbzSession.tr("Last.fm and ListenBrainz settings, including their credentials.", QbzSession.trRev)
        QbzToggle {
            checked: root.copyScrobblers
            onToggled: function (v) { root.copyScrobblers = v }
        }
    }
    SettingRow {
        rowEnabled: root.copyLocalProfile
        label: QbzSession.tr("Listening history", QbzSession.trRev)
        description: QbzSession.tr("The listen log and the events behind offline recommendations.", QbzSession.trRev)
        QbzToggle {
            checked: root.copyListeningHistory
            onToggled: function (v) { root.copyListeningHistory = v }
        }
    }
    Repeater {
        model: root.ie.snapshots || []
        delegate: SettingRow {
            required property var modelData
            label: modelData.label
            description: QbzSession.tr("{} favorites · {} playlists · {} followed", QbzSession.trRev)
                .replace("{}", modelData.favorites)
                .replace("{}", modelData.playlists)
                .replace("{}", modelData.subscriptions)
                + (modelData.isCurrentAccount
                    ? " · " + QbzSession.tr("this account", QbzSession.trRev)
                    : "")
            SettingsButton {
                text: QbzSession.tr("Migrate…", QbzSession.trRev)
                enabled: !root.ie.migrationBusy && !modelData.isCurrentAccount
                onClicked: QbzBridge.settingsString("account-migrate", JSON.stringify({
                    path: modelData.path,
                    local_profile: root.copyLocalProfile,
                    media_servers: root.copyMediaServers,
                    scrobblers: root.copyScrobblers,
                    listening_history: root.copyListeningHistory
                }))
            }
        }
    }
    Text {
        visible: (root.ie.migrationStatus || "") !== ""
        width: parent.width
        text: root.ie.migrationStatus || ""
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }
}
