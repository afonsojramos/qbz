// Settings > Local Library > PLEX — the QML port of the PLEX half of
// crates/qbz-ui/ui/settings/LocalLibrarySettings.slint (lines 517-855):
// master toggle + collapse header, server address, token, connection
// actions, the music-library picker, the metadata-write toggle and the Plex
// danger zone.
//
// Wiring: the LIVE state (enabled / available / syncing / sections / error)
// is the QbzLocal bridge's own Plex properties — the SAME ones the Local
// Library browse view reads, so a change here reloads the browse union in
// one hop. The two fields the bridge does not publish (the persisted server
// url and whether a token is stored) ride the settings document
// (settings_qt/library.rs). The token itself NEVER leaves Rust.
//
// Delta vs the Slint: the PIN flow (Authorize / Generate code / Link code /
// Copy code / Open Plex sign-in, and the "Enter token manually" switch that
// chooses between them) is NOT ported — `local_plex.rs` implements the
// manual server-url + token path only, so that path is always shown and the
// PIN rows are absent instead of dead.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var plex: (doc.library || ({})).plex || ({})

    // In-progress field values (the fields commit on Enter / focus loss; the
    // Connect button submits whatever is typed right now).
    property string urlInput: ""
    property string tokenInput: ""

    QbzTheme { id: theme }

    spacing: 4

    readonly property var sections: {
        try {
            return JSON.parse(QbzLocal.plexSectionsJson)
        } catch (e) {
            return []
        }
    }

    // Same lazy-load hook as the Slint panel's init: re-publish the Plex
    // state (sections + gates) whenever the section becomes visible.
    onVisibleChanged: {
        if (visible) {
            urlInput = plex.serverUrl || ""
            QbzLocal.refreshPlex()
        }
    }

    // ===================== header: title + toggle + chevron ================
    Item {
        width: parent.width
        height: 64
        Column {
            anchors.left: parent.left
            anchors.right: plexControls.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: QbzSession.tr("PLEX", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 11
                font.letterSpacing: 1.5
                font.weight: theme.weightSemibold
            }
            Text {
                width: parent.width
                text: QbzSession.tr("Connect a Plex Media Server on your local network.", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }
        }
        Row {
            id: plexControls
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Rectangle {
                visible: QbzLocal.plexEnabled
                width: 28
                height: 28
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: chevArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: root.plex.collapsed === true ? "chevron-right" : "chevron-down"
                    width: 18
                    height: 18
                    tintName: "secondary"
                }
                MouseArea {
                    id: chevArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.settingsBool("plex-collapse", root.plex.collapsed !== true)
                }
            }
            QbzToggle {
                anchors.verticalCenter: parent.verticalCenter
                checked: QbzLocal.plexEnabled
                onToggled: function (v) { QbzLocal.plexSetEnabled(v) }
            }
        }
    }

    // ============================== body ==================================
    Column {
        visible: QbzLocal.plexEnabled && root.plex.collapsed !== true
        width: parent.width
        spacing: 4

        Item { width: 1; height: 10 }

        SettingRow {
            label: QbzSession.tr("Server address", QbzSession.trRev)
            description: QbzSession.tr("Only local network servers are supported.", QbzSession.trRev)
            QbzLineEdit {
                id: urlField
                width: 240
                text: root.plex.serverUrl || ""
                placeholder: "http://127.0.0.1:32400"
                onEdited: function (v) { root.urlInput = v }
                onCommitted: function (v) { root.urlInput = v }
            }
        }
        SettingRow {
            label: QbzSession.tr("Token", QbzSession.trRev)
            description: root.plex.hasToken === true
                ? QbzSession.tr("A token is stored for this server.", QbzSession.trRev)
                : QbzSession.tr("Use an existing X-Plex-Token.", QbzSession.trRev)
            QbzLineEdit {
                id: tokenField
                width: 240
                isPassword: true
                placeholder: QbzSession.tr("X-Plex-Token", QbzSession.trRev)
                onEdited: function (v) { root.tokenInput = v }
                onCommitted: function (v) { root.tokenInput = v }
            }
        }
        // Connect = persist the credentials + sync (plex_connect resolves the
        // url, refuses anything that is not a LAN address, then runs a sync).
        SettingRow {
            label: QbzSession.tr("Connect", QbzSession.trRev)
            description: QbzSession.tr("Saves the address and token, then fetches your libraries.", QbzSession.trRev)
            SettingsButton {
                text: QbzLocal.plexSyncing
                    ? QbzSession.tr("Working...", QbzSession.trRev)
                    : QbzSession.tr("Connect", QbzSession.trRev)
                enabled: !QbzLocal.plexSyncing
                onClicked: {
                    QbzLocal.plexConnect(root.urlInput !== "" ? root.urlInput
                        : (root.plex.serverUrl || ""), root.tokenInput)
                    root.tokenInput = ""
                    tokenField.text = ""
                }
            }
        }

        Item { width: 1; height: 14 }

        SettingRow {
            label: QbzSession.tr("Libraries", QbzSession.trRev)
            description: QbzSession.tr("Fetch your Plex music libraries.", QbzSession.trRev)
            SettingsButton {
                text: QbzLocal.plexSyncing
                    ? QbzSession.tr("Working...", QbzSession.trRev)
                    : QbzSession.tr("Get libraries", QbzSession.trRev)
                enabled: QbzLocal.plexAvailable && !QbzLocal.plexSyncing
                onClicked: QbzLocal.syncPlex()
            }
        }

        // Status line: the last Plex error, or the last sync's track count.
        Text {
            visible: QbzLocal.plexError !== "" || QbzLocal.plexLastSyncTracks >= 0
            width: parent.width
            text: QbzLocal.plexError !== ""
                ? QbzLocal.plexError
                : QbzSession.tr("Synced {} tracks", QbzSession.trRev)
                    .replace("{}", QbzLocal.plexLastSyncTracks)
            color: QbzLocal.plexError !== "" ? theme.danger : theme.success
            font.pixelSize: theme.fontLegal
            wrapMode: Text.WordWrap
        }

        Item { width: 1; height: 6 }

        // ---------------------- music library picker ----------------------
        Column {
            visible: root.sections.length > 0
            width: parent.width
            spacing: 2
            GroupHeader { text: QbzSession.tr("MUSIC LIBRARIES", QbzSession.trRev) }
            Item { width: 1; height: 4 }
            Repeater {
                model: root.sections
                delegate: Rectangle {
                    id: secRow
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 42
                    radius: theme.radiusSm
                    color: secArea.containsMouse ? theme.surfaceHover : "transparent"

                    function toggle() {
                        const keys = []
                        for (let i = 0; i < root.sections.length; i++) {
                            const s = root.sections[i]
                            const on = s.key === secRow.modelData.key ? !s.selected : s.selected
                            if (on) keys.push(s.key)
                        }
                        QbzLocal.setPlexSections(JSON.stringify(keys))
                    }

                    MouseArea {
                        id: secArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: secRow.toggle()
                    }
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 12
                        spacing: 12
                        QbzCheckbox {
                            anchors.verticalCenter: parent.verticalCenter
                            checked: secRow.modelData.selected === true
                            onToggled: secRow.toggle()
                        }
                        Text {
                            width: parent.width - 18 - 24
                            height: parent.height
                            text: secRow.modelData.title || ""
                            color: theme.textPrimary
                            font.pixelSize: theme.fontBody
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                    }
                }
            }
            Item { width: 1; height: 14 }
        }

        // -------------------------- metadata write -------------------------
        SettingRow {
            label: QbzSession.tr("Write metadata to Plex (experimental)", QbzSession.trRev)
            description: QbzSession.tr("Allow QBZ to write track metadata back to your Plex server.", QbzSession.trRev)
            QbzToggle {
                checked: root.plex.metadataWrite === true
                onToggled: function (v) { QbzBridge.settingsBool("plex-metadata-write", v) }
            }
        }

        Item { width: 1; height: 18 }

        // ------------------------- plex danger zone ------------------------
        GroupHeader { text: QbzSession.tr("PLEX DANGER ZONE", QbzSession.trRev) }
        SettingRow {
            label: QbzSession.tr("Disconnect", QbzSession.trRev)
            description: QbzSession.tr("Sign out of Plex and clear the local cache.", QbzSession.trRev)
            SettingsButton {
                danger: true
                text: QbzSession.tr("Disconnect", QbzSession.trRev)
                enabled: root.plex.hasToken === true
                onClicked: QbzLocal.plexDisconnect()
            }
        }
        SettingRow {
            label: QbzSession.tr("Clear cache", QbzSession.trRev)
            description: QbzSession.tr("Remove cached Plex libraries and tracks. Your sign-in is kept.", QbzSession.trRev)
            SettingsButton {
                danger: true
                text: QbzSession.tr("Clear cache", QbzSession.trRev)
                enabled: root.plex.hasToken === true
                onClicked: QbzBridge.settingsString("plex-clear-cache", "")
            }
        }
    }
}
