// Settings > Developer — the QML port of crates/qbz-ui/ui/settings/
// DeveloperSettings.slint.
//
// Shipped, in the Slint's order: QOBUZ CONNECT · LOGS · SETTINGS PORTABILITY.
//
// Deltas vs the Slint (no dead rows):
// - LOGS offers TWO buttons where the Slint has one: "View logs" opens the
//   in-app viewer (copy / redact / upload) and "Open log file" hands the
//   on-disk sink to the desktop.
// - "Export settings…" carries its include-auth gate INLINE (the
//   "Include sign-in credentials" switch, default OFF and reset on hide)
//   instead of inside a SettingsExportModal. The modal is a wrapper; the
//   security decision it guards is the same one, in the same place.
// - The inline DiagnosticsPanel is ported and mounted last, as in the
//   reference — with the GTK/WebKit/GSK/GDK rows CUT, because in a Qt process
//   they can only read "—" and their saved column would come from a file this
//   binary never writes. The panel's own header explains the cut.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var dev: doc.dev || ({})

    QbzTheme { id: theme }

    spacing: 4

    // ========================== QOBUZ CONNECT ============================
    GroupHeader { text: QbzSession.tr("QOBUZ CONNECT", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Connect diagnostics", QbzSession.trRev)
        description: QbzSession.tr("Live session topology and a rolling event log for debugging Qobuz Connect at runtime.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Open diagnostics", QbzSession.trRev)
            onClicked: QbzQConnect.diagSetOpen(true)
        }
    }

    SettingsSpacer { }

    // ============================== LOGS =================================
    GroupHeader { text: QbzSession.tr("LOGS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Application logs", QbzSession.trRev)
        description: (root.dev.logPath || "") !== ""
            ? root.dev.logPath
            : QbzSession.tr("View the in-app log, copy it (secrets redacted), or upload it to share in a GitHub issue.", QbzSession.trRev)
        Row {
            spacing: 8
            // The description above has always promised "view the in-app log,
            // copy it (secrets redacted), or upload it" — until the viewer
            // landed, the only button here did none of those three.
            SettingsButton {
                text: QbzSession.tr("View logs", QbzSession.trRev)
                onClicked: QbzShell.logOpen()
            }
            SettingsButton {
                text: QbzSession.tr("Open log file", QbzSession.trRev)
                enabled: (root.dev.logPath || "") !== ""
                onClicked: QbzBridge.settingsString("open-log-file", "")
            }
        }
    }

    SettingsSpacer { }

    // ====================== SETTINGS PORTABILITY =========================
    GroupHeader { text: QbzSession.tr("SETTINGS PORTABILITY", QbzSession.trRev) }
    // The `--include-auth` gate. The reference puts it in a modal
    // (SettingsExportModal.slint) with ONE checkbox, default OFF; this port
    // exports directly from the row, so the checkbox is a row too.
    //
    // DELIBERATELY NOT PERSISTED. The reference's default-OFF is re-asserted
    // every time its modal opens, so the user re-decides per export. A
    // persisted toggle would not: switch it on once and every later export
    // silently carries credentials. `includeAuth` therefore lives here as
    // session state and resets whenever the panel is left.
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
        visible: (root.dev.status || "") !== ""
        width: parent.width
        text: root.dev.status || ""
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        wrapMode: Text.WordWrap
    }

    SettingsSpacer { }

    // ========================== DIAGNOSTICS ==============================
    // Last child, 1:1 with DeveloperSettings.slint:92.
    DiagnosticsPanel { }
}
