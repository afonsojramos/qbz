// Settings > Developer — the QML port of crates/qbz-ui/ui/settings/
// DeveloperSettings.slint.
//
// Shipped, in the Slint's order: LOGS · SETTINGS PORTABILITY.
//
// Deltas vs the Slint (no dead rows):
// - "QOBUZ CONNECT / Connect diagnostics" is absent: this port has no live
//   Qobuz Connect service, so the diagnostics modal would have no session to
//   describe.
// - LOGS opens the on-disk log FILE (qbz_log's file sink) instead of the
//   in-app log viewer overlay (copy / redact / upload), which is not ported.
// - "Export settings…" writes the bundle immediately with auth EXCLUDED; the
//   SettingsExportModal's include-auth gate (default OFF) is not ported, so
//   the safe default is the only behaviour.
// - The inline DiagnosticsPanel (seven saved-vs-runtime tables) is not
//   ported — it is a view of its own, not a settings row.

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

    // ============================== LOGS =================================
    GroupHeader { text: QbzSession.tr("LOGS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Application logs", QbzSession.trRev)
        description: (root.dev.logPath || "") !== ""
            ? root.dev.logPath
            : QbzSession.tr("View the in-app log, copy it (secrets redacted), or upload it to share in a GitHub issue.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Open log file", QbzSession.trRev)
            enabled: (root.dev.logPath || "") !== ""
            onClicked: QbzBridge.settingsString("open-log-file", "")
        }
    }

    SettingsSpacer { }

    // ====================== SETTINGS PORTABILITY =========================
    GroupHeader { text: QbzSession.tr("SETTINGS PORTABILITY", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Export settings…", QbzSession.trRev)
        description: QbzSession.tr("Save a portable bundle of your settings to move to another machine or the qbzd daemon.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Export…", QbzSession.trRev)
            onClicked: QbzBridge.settingsString("export-settings", "")
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
}
