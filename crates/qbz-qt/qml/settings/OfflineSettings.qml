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
// "Check now" IS shipped (`offline-recheck`); so is the live tri-state status
// line (QbzSession.offlineMode / captivePortal) and the lyrics-cache row.
//
// Still missing: the whole OFFLINE CACHE group (Open manager / Open folder /
// Clear all). What blocks it is the MANAGER VIEW, which has no Qt counterpart
// yet — not the engine: `offline_cache_qt.rs` already runs the downloads from
// the album page and the track row.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var off: doc.offline || ({})
    /// The view-level SettingsConfirmHost (SettingsView.qml). Null in previews
    /// — the Clear-all row guards, so a preview degrades to the unconfirmed
    /// call rather than swallowing the click.
    property var confirmHost: null

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
    // Ask the connectivity actor for an immediate probe rather than waiting
    // for its next scheduled one. Disabled under MANUAL offline (mode 2),
    // where the answer is a user decision and not a network fact.
    SettingRow {
        label: QbzSession.tr("Check connection", QbzSession.trRev)
        description: QbzSession.tr("Test the connection now instead of waiting for the next automatic check.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Check now", QbzSession.trRev)
            enabled: QbzSession.offlineMode !== 2
            onClicked: QbzBridge.settingsString("offline-recheck", "")
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================== OFFLINE CACHE ============================
    // The downloads half: the manager view plus the two whole-cache actions.
    // All three ride QbzOffline, the same bridge the manager view uses —
    // "Open folder" and "Clear all" are the manager's own stats-bar buttons,
    // offered here too exactly as the reference offers them
    // (OfflineSettings.slint:135-167).
    GroupHeader { text: QbzSession.tr("OFFLINE CACHE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Manage offline cache", QbzSession.trRev)
        description: QbzSession.tr("Browse and manage your downloaded tracks and albums.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Open manager", QbzSession.trRev)
            onClicked: QbzOffline.openManager()
        }
    }
    SettingRow {
        label: QbzSession.tr("Cache folder", QbzSession.trRev)
        description: QbzSession.tr("Open the folder where offline tracks are stored on disk.", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Open folder", QbzSession.trRev)
            onClicked: QbzOffline.openFolder()
        }
    }
    SettingRow {
        label: QbzSession.tr("Clear cache", QbzSession.trRev)
        description: QbzSession.tr("Frees up cached data. Your downloaded albums are kept — remove those from the offline manager above.", QbzSession.trRev)
        SettingsButton {
            danger: true
            text: QbzSession.tr("Clear all", QbzSession.trRev)
            // ONE prompt before the purge. The reference fires straight from
            // the button; this port confirms every destructive settings row,
            // and undoing this one means re-downloading the whole cache.
            onClicked: {
                if (!root.confirmHost) {
                    QbzOffline.clearAll()
                    return
                }
                root.confirmHost.ask(
                    QbzSession.tr("Clear cache", QbzSession.trRev),
                    QbzSession.tr("Frees up cached data. Your downloaded albums are kept — remove those from the offline manager above.", QbzSession.trRev),
                    QbzSession.tr("Clear all", QbzSession.trRev),
                    function () { QbzOffline.clearAll() })
            }
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
