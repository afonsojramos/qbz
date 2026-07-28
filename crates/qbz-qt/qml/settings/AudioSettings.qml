// Settings > Audio — the QML port of crates/qbz-ui/ui/settings/
// AudioSettings.slint. Group order, labels, descriptions, gating and control
// types are 1:1 with the Slint; every control rides the single settingsJson
// document (root.doc) and the settingsBool/Select invokables — never local
// truth. The audio path itself is PROTECTED: this panel only calls the
// settings keys settings_qt.rs already dispatches.
//
// Not shipped (no backend seam in this port — see settings_qt.rs):
// - "Detected device limit" + its fallback disclosure (#638): the device-cap
//   probe (`device_cap.rs`) is Slint glue that was never ported, so there is
//   no measured value to show.
// - "HiFi Wizard": the guided PipeWire setup (DacWizardActions) is a whole
//   modal flow, not a settings row.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // ============================ STREAMING ==============================
    GroupHeader { text: QbzSession.tr("STREAMING", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Streaming quality", QbzSession.trRev)
        description: QbzSession.tr("The quality tier QBZ requests for playback.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.streamingQualities || []
            currentIndex: root.doc.streamingQualityIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("streaming-quality", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Limit quality to device", QbzSession.trRev)
        description: QbzSession.tr("Cap the requested streaming quality at your output device's limit. Applies to local playback only, never to casting.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.limitQualityToDevice === true
            onToggled: function (v) { QbzBridge.settingsBool("limit-quality-to-device", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ============================= OUTPUT ================================
    GroupHeader { text: QbzSession.tr("OUTPUT", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Audio backend", QbzSession.trRev)
        description: QbzSession.tr("The audio stack QBZ routes playback through.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.backends || []
            currentIndex: root.doc.backendIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("backend", i) }
        }
    }
    // JACK routes through the JACK graph (which resamples to the graph rate),
    // so it can never be bit-perfect — say so instead of letting the
    // bit-perfect toggles below imply otherwise.
    WarningBanner {
        visible: root.doc.backendIsJack === true
        variant: "warning"
        title: QbzSession.tr("JACK is not bit-perfect", QbzSession.trRev)
        body: QbzSession.tr("The JACK backend routes audio through the JACK graph, which resamples to the graph's sample rate. For bit-perfect playback use ALSA (direct) or PipeWire with passthrough.", QbzSession.trRev)
    }
    SettingRow {
        label: QbzSession.tr("Output device", QbzSession.trRev)
        description: QbzSession.tr("The DAC or sound device that receives audio.", QbzSession.trRev)
        Row {
            spacing: 8
            // Refresh / release: frees a device QBZ holds exclusively (ALSA
            // direct) and re-enumerates, so a freed or hot-plugged DAC shows
            // up without an app restart.
            SettingsButton {
                iconName: "refresh-cw"
                onClicked: QbzBridge.refreshDevices()
            }
            QbzSelect {
                menuWidth: 300
                popupWidth: 480
                searchable: true
                options: root.doc.devices || []
                currentIndex: root.doc.deviceIndex || 0
                onSelected: function (i) { QbzBridge.settingsSelect("device", i) }
            }
        }
    }
    SettingRow {
        visible: root.doc.backendIsAlsa === true
        label: QbzSession.tr("ALSA plugin", QbzSession.trRev)
        description: QbzSession.tr("How ALSA opens the device — hw is bit-perfect, plughw converts.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.alsaPlugins || []
            currentIndex: root.doc.alsaPluginIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("alsa-plugin", i) }
        }
    }
    SettingRow {
        visible: root.doc.backendIsAlsa === true && root.doc.alsaPluginIsHw === true
        label: QbzSession.tr("Hardware volume control", QbzSession.trRev)
        description: QbzSession.tr("Use the ALSA mixer for volume instead of software gain.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.alsaHardwareVolume === true
            onToggled: function (v) { QbzBridge.settingsBool("alsa-hardware-volume", v) }
        }
    }
    SettingRow {
        visible: root.doc.backendIsAlsa === true
        label: QbzSession.tr("DSD playback", QbzSession.trRev)
        description: QbzSession.tr("How DSD tracks reach the DAC. WARNING: choose DoP or Native only if your DAC supports it — on any other DAC they play as loud noise. Volume is fixed and seeking is disabled in DoP/Native mode. Native additionally needs kernel support for the DAC.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 280
            options: root.doc.dsdModes || []
            currentIndex: root.doc.dsdModeIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("dsd-mode", i) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // =========================== BIT-PERFECT =============================
    GroupHeader { text: QbzSession.tr("BIT-PERFECT", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Exclusive mode", QbzSession.trRev)
        description: QbzSession.tr("Lock the device so no other app can resample it.", QbzSession.trRev)
        rowEnabled: root.doc.backendIsAlsa === true
        QbzToggle {
            checked: root.doc.exclusiveMode === true
            enabled: root.doc.backendIsAlsa === true
            onToggled: function (v) { QbzBridge.settingsBool("exclusive-mode", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Reserve DAC while running", QbzSession.trRev)
        description: QbzSession.tr("Hold the device reserved so other apps can't grab it.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.reserveDac === true
            onToggled: function (v) { QbzBridge.settingsBool("reserve-dac", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("DAC passthrough", QbzSession.trRev)
        description: QbzSession.tr("Send the bitstream untouched to the DAC.", QbzSession.trRev)
        rowEnabled: root.doc.backendIsPipewire === true
        QbzToggle {
            checked: root.doc.dacPassthrough === true
            enabled: root.doc.backendIsPipewire === true
            onToggled: function (v) { QbzBridge.settingsBool("dac-passthrough", v) }
        }
    }
    SettingRow {
        visible: root.doc.dacPassthrough === true && root.doc.backendIsPipewire === true
        label: QbzSession.tr("Force bit-perfect", QbzSession.trRev)
        description: QbzSession.tr("Pin the PipeWire quantum and rate for the active track.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.pwForceBitperfect === true
            onToggled: function (v) { QbzBridge.settingsBool("pw-force-bitperfect", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Allow quality fallback", QbzSession.trRev)
        description: QbzSession.tr("Drop to a lower tier when the requested one is unavailable.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.allowQualityFallback === true
            onToggled: function (v) { QbzBridge.settingsBool("allow-quality-fallback", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ============================= STARTUP ===============================
    GroupHeader { text: QbzSession.tr("STARTUP", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Sync audio settings on startup", QbzSession.trRev)
        description: QbzSession.tr("Reload saved audio settings into the player when QBZ launches.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.syncAudioOnStartup === true
            onToggled: function (v) { QbzBridge.settingsBool("sync-audio-on-startup", v) }
        }
    }
    SettingRow {
        visible: root.doc.backendIsPipewire === true
        label: QbzSession.tr("Lock output device", QbzSession.trRev)
        description: QbzSession.tr("Keep external routing intact — skip switching the default sink.", QbzSession.trRev)
        rowEnabled: root.doc.dacPassthrough !== true
        QbzToggle {
            checked: root.doc.skipSinkSwitch === true
            enabled: root.doc.dacPassthrough !== true
            onToggled: function (v) { QbzBridge.settingsBool("skip-sink-switch", v) }
        }
    }

    Item { width: 1; height: 20 }

    // Reset — restores Audio + Playback defaults.
    SettingsButton {
        text: QbzSession.tr("Reset to defaults", QbzSession.trRev)
        onClicked: QbzBridge.settingsReset()
    }
}
