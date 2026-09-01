// Settings > Playback — the QML port of crates/qbz-ui/ui/settings/
// PlaybackSettings.slint. 1:1 group order, labels, descriptions and gating;
// every control rides the settingsJson document and the settings invokables.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // ============================ PLAYBACK ===============================
    GroupHeader { text: QbzSession.tr("PLAYBACK", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Continue playback after track ends", QbzSession.trRev)
        description: QbzSession.tr("Keep playing the rest of the album or playlist instead of stopping.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.continuePlayback === true
            onToggled: function (v) { QbzBridge.settingsBool("continue-playback", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show track playing context", QbzSession.trRev)
        description: QbzSession.tr("Display the context-stack icon in the player.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.showContextIcon === true
            onToggled: function (v) { QbzBridge.settingsBool("show-context-icon", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Gapless playback", QbzSession.trRev)
        description: QbzSession.tr("Play consecutive same-format tracks without a gap.", QbzSession.trRev)
        // Disabled while Streaming only is on — gapless needs the local cache.
        rowEnabled: root.doc.streamingOnly !== true
        QbzToggle {
            checked: root.doc.gapless === true
            enabled: root.doc.streamingOnly !== true
            onToggled: function (v) { QbzBridge.settingsBool("gapless", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ============================= SESSION ===============================
    GroupHeader { text: QbzSession.tr("SESSION", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Restore session on startup", QbzSession.trRev)
        description: QbzSession.tr("Restore the queue and current track on the next launch.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.persistSession === true
            onToggled: function (v) { QbzBridge.settingsBool("persist-session", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Resume playback position", QbzSession.trRev)
        description: QbzSession.tr("Also seek back to where the saved track left off.", QbzSession.trRev)
        rowEnabled: root.doc.persistSession === true
        QbzToggle {
            checked: root.doc.resumePosition === true
            enabled: root.doc.persistSession === true
            onToggled: function (v) { QbzBridge.settingsBool("resume-position", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Auto-connect Qobuz Connect on startup", QbzSession.trRev)
        description: QbzSession.tr("Choose whether Qobuz Connect activates automatically when QBZ launches.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 240
            options: root.doc.qconnectStartupModes || []
            currentIndex: root.doc.qconnectStartupIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("qconnect-startup", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("When playback conflicts", QbzSession.trRev)
        description: QbzSession.tr("Ask each time or automatically choose which queue and device should continue.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 300
            popupWidth: 420
            options: root.doc.qconnectConflictPolicies || []
            currentIndex: root.doc.qconnectConflictPolicyIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("qconnect-conflict-policy", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Qobuz Connect device name", QbzSession.trRev)
        description: QbzSession.tr("The name other Qobuz Connect apps see for this device. Applies on the next connection.", QbzSession.trRev)
        QbzLineEdit {
            width: 240
            text: root.doc.qconnectDeviceName || ""
            placeholder: root.doc.qconnectDeviceNameDefault || ""
            onCommitted: function (s) { QbzBridge.settingsString("qconnect-device-name", s) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ============================ STREAMING ==============================
    GroupHeader { text: QbzSession.tr("STREAMING", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Stream uncached tracks", QbzSession.trRev)
        description: QbzSession.tr("Start uncached tracks via streaming instead of waiting for the full download.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.streamUncached === true
            onToggled: function (v) { QbzBridge.settingsBool("stream-uncached", v) }
        }
    }
    SettingRow {
        visible: root.doc.streamUncached === true
        label: QbzSession.tr("Initial buffer size", QbzSession.trRev)
        description: QbzSession.tr("Seconds of audio buffered before streaming playback starts.", QbzSession.trRev)
        Row {
            spacing: 12
            QbzSlider {
                width: 160
                anchors.verticalCenter: parent.verticalCenter
                minimum: 1
                maximum: 10
                value: root.doc.bufferSeconds || 1
                onChanged: function (v) { QbzBridge.settingsSlider("buffer-seconds", v) }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: (root.doc.bufferSeconds || 1) + QbzSession.tr("s", QbzSession.trRev)
                color: theme.textSecondary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
            }
        }
    }
    SettingRow {
        label: QbzSession.tr("Streaming only", QbzSession.trRev)
        description: QbzSession.tr("Skip writing tracks to the local cache while streaming.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.streamingOnly === true
            onToggled: function (v) { QbzBridge.settingsBool("streaming-only", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("When quality retries fail", QbzSession.trRev)
        description: QbzSession.tr("What to do when every quality tier for a track is unavailable.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 240
            options: root.doc.retryBehaviors || []
            currentIndex: root.doc.retryBehaviorIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("retry-behavior", i) }
        }
    }
}
