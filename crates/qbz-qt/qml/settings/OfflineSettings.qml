// Settings > Offline — the QML port of crates/qbz-ui/ui/settings/
// OfflineSettings.slint.
//
// OFFLINE MODE (the app running without Qobuz) is the shared engine: the
// status line reads the LIVE QbzSession properties the engine forwarder
// publishes (tri-state online / no connection / induced + captive portal),
// and the toggle rides settingsBool("offline-mode-enabled") into
// `OfflineModeEngine::set_induced` (which also takes the #279 stream-first
// snapshot).
//
// Not shipped (no backend seam in this port — see settings_qt/offline.rs):
// - "Check now": the connectivity actor is private to offline_fwd.rs, so a
//   re-probe cannot be triggered from here. The status itself is live.
// - The whole OFFLINE CACHE group (Open manager / Open folder / Clear all):
//   the port never brings up OfflineCacheState, so all three would be dead.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var off: doc.offline || ({})

    QbzTheme { id: theme }

    spacing: 4

    // ========================== OFFLINE MODE =============================
    GroupHeader { text: QbzSession.tr("OFFLINE MODE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Status", QbzSession.trRev)
        description: QbzSession.captivePortal
            ? QbzSession.tr("Captive portal detected — sign in to the network to get online.", QbzSession.trRev)
            : ""
        Text {
            // Tri-state: online green / real-offline amber / induced accent.
            text: QbzSession.offlineMode === 2 ? QbzSession.tr("Offline mode enabled", QbzSession.trRev)
                : QbzSession.offlineMode === 1 ? QbzSession.tr("No connection", QbzSession.trRev)
                : QbzSession.tr("Online", QbzSession.trRev)
            color: QbzSession.offlineMode === 2 ? theme.accent
                : QbzSession.offlineMode === 1 ? theme.warning : theme.success
            font.pixelSize: theme.fontBody
            font.weight: theme.weightMedium
        }
    }
    SettingRow {
        label: QbzSession.tr("Enable Offline Mode", QbzSession.trRev)
        description: QbzSession.tr("Manually switch to offline mode even with internet.", QbzSession.trRev)
        QbzToggle {
            checked: root.off.modeEnabled === true
            onToggled: function (v) { QbzBridge.settingsBool("offline-mode-enabled", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ============================= LYRICS ================================
    GroupHeader { text: QbzSession.tr("LYRICS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Clear lyrics cache", QbzSession.trRev)
        // "{} entries using {}" — the real per-user lyrics.db stats.
        description: root.off.lyricsLoaded === true
            ? QbzSession.tr("{} entries using {}", QbzSession.trRev)
                .replace("{}", root.off.lyricsEntries)
                .replace("{}", root.off.lyricsSize)
            : ""
        SettingsButton {
            danger: true
            text: QbzSession.tr("Clear", QbzSession.trRev)
            onClicked: QbzBridge.settingsString("lyrics-cache-clear", "")
        }
    }
}
