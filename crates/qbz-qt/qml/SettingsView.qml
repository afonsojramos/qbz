// Settings view — the QML port of crates/qbz-ui/ui/settings/
// SettingsView.slint + AudioSettings.slint + PlaybackSettings.slint:
// a 92px title header, a 232px left sub-navigation, and the active panel
// in a touch-draggable Flickable with a ListScrollbar replica.
//
// All state is ONE JSON document (QbzBridge.settingsJson, settings_qt.rs
// SettingsDoc). Controls never keep local truth: they call the
// settingsBool/Select/Slider/String invokables, Rust persists + applies +
// republishes, and the rows re-render from the new document (the Slint
// "single source of truth in SettingsState" pattern).
//
// POC-NOTEs (deliberate cuts, named for the effort report):
// - Sections: Audio / Playback / Appearance / Integrations render live
//   (phase 19); Offline / Local Library / Blacklist / Developer render an
//   honest placeholder page with the exact section header; the conditional
//   Flatpak/Snap entry and the "Share logs" LogViewer are not ported.
// - The NavButtons row inside the 92px header is a 0px placeholder (nav
//   history lives in the global HeaderBar here, like every other view).
// - Audio > "Detected device limit" read-only row: skipped — the #638
//   device-cap probe glue (device_cap_summary) is not ported.
// - Audio > JACK "not bit-perfect" WarningBanner: skipped (POC-NOTE; the
//   backendIsJack flag IS published for a future port).
// - Audio > HiFi Wizard button: skipped (DacWizardActions not ported).
// - Settings export/import modal: not ported.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Item {
    id: root

    QbzTheme { id: theme }

    // The parsed settingsJson document ({}) until the first publish lands).
    property var doc: ({})
    // Active sub-section (display order): 0 Audio, 1 Playback, 2 Appearance,
    // 3 Offline, 4 Local Library, 5 Blacklist, 6 Integrations, 7 Developer.
    property int section: 0

    function reload() {
        try {
            root.doc = JSON.parse(QbzBridge.settingsJson)
        } catch (e) {
            root.doc = ({})
        }
    }
    Component.onCompleted: reload()
    Connections {
        target: QbzBridge
        function onSettingsJsonChanged() { root.reload() }
    }

    // ============================ the view ================================
    Column {
        anchors.fill: parent
        spacing: 0

        // --- Header (92px; NavButtons is a 0px placeholder in this port) --
        Item {
            width: parent.width
            height: 92
            Text {
                x: 32
                // padding-top 11 + 12px gap below the (0px) NavButtons row.
                y: 23
                text: QbzBridge.tr("Settings")
                color: theme.textPrimary
                font.pixelSize: theme.fontTitle
                font.weight: theme.weightBold
            }
        }

        // --- Sub-nav + active panel ---------------------------------------
        Row {
            width: parent.width
            height: parent.height - 92

            // Left sub-navigation (232px).
            Item {
                width: 232
                height: parent.height
                Column {
                    anchors.fill: parent
                    anchors.leftMargin: 24
                    anchors.rightMargin: 12
                    anchors.topMargin: 4
                    anchors.bottomMargin: 16
                    spacing: 4

                    component SubNavItem: Rectangle {
                        property string name: ""
                        property string label: ""
                        property bool active: false
                        signal clicked()

                        width: parent ? parent.width : 0
                        height: 38
                        radius: theme.radiusSm
                        color: active ? theme.surfaceElevated
                            : snArea.containsMouse ? theme.surfaceHover : "transparent"
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 10
                            QbzIcon {
                                name: parent.parent.name
                                width: 16
                                height: 16
                                anchors.verticalCenter: parent.verticalCenter
                                tintName: parent.parent.active ? "primary" : "secondary"
                            }
                            Text {
                                height: parent.height
                                text: parent.parent.label
                                color: parent.parent.active ? theme.textPrimary : theme.textSecondary
                                font.pixelSize: theme.fontBody
                                font.weight: parent.parent.active ? theme.weightSemibold : theme.weightRegular
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                        MouseArea {
                            id: snArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: parent.clicked()
                        }
                    }

                    SubNavItem {
                        name: "volume-2"
                        label: QbzBridge.tr("Audio")
                        active: root.section === 0
                        onClicked: root.section = 0
                    }
                    SubNavItem {
                        name: "play-fill"
                        label: QbzBridge.tr("Playback")
                        active: root.section === 1
                        onClicked: root.section = 1
                    }
                    SubNavItem {
                        name: "layers"
                        label: QbzBridge.tr("Appearance")
                        active: root.section === 2
                        onClicked: root.section = 2
                    }
                    SubNavItem {
                        name: "cloud-download"
                        label: QbzBridge.tr("Offline")
                        active: root.section === 3
                        onClicked: root.section = 3
                    }
                    SubNavItem {
                        name: "hard-drive"
                        label: QbzBridge.tr("Local Library")
                        active: root.section === 4
                        onClicked: root.section = 4
                    }
                    SubNavItem {
                        name: "blind-eye"
                        label: QbzBridge.tr("Blacklist")
                        active: root.section === 5
                        onClicked: root.section = 5
                    }
                    SubNavItem {
                        name: "refresh-cw"
                        label: QbzBridge.tr("Integrations")
                        active: root.section === 6
                        onClicked: root.section = 6
                    }
                    SubNavItem {
                        name: "bug"
                        label: QbzBridge.tr("Developer")
                        active: root.section === 7
                        onClicked: root.section = 7
                    }
                    // POC-NOTE: the conditional Flatpak/Snap sub-nav entry
                    // (SandboxState.install-method) is omitted — the POC is
                    // not a sandboxed install.

                    // "Share logs" footer (SettingsView.slint:202-215),
                    // pinned to the subnav bottom.
                    Item { width: 1; height: parent.height - 8 * 42 - 38 - 4 }
                    SubNavItem {
                        name: "cloud-upload"
                        label: QbzBridge.tr("Share logs")
                        active: false
                        // POC-NOTE: LogViewerState (copy + upload viewer) is
                        // not ported — the row is inert.
                        onClicked: { }
                    }
                }
            }

            // Active panel — a raw Flickable (touch-drag scroll on the Pi
            // kiosk, per the Slint comment) + a ListScrollbar replica.
            Item {
                width: parent.width - 232
                height: parent.height

                Flickable {
                    id: flick
                    anchors.fill: parent
                    contentWidth: width
                    contentHeight: panelCol.height + 60
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: panelCol
                        x: 20
                        y: 4
                        width: flick.width - 60 // 20 left + 40 right padding
                        spacing: 4

                        // ===================== AUDIO ======================
                        Column {
                            visible: root.section === 0
                            width: parent.width
                            spacing: 4

                            GroupHeader { text: QbzBridge.tr("STREAMING") }
                            SettingRow {
                                label: QbzBridge.tr("Streaming quality")
                                description: QbzBridge.tr("The quality tier QBZ requests for playback.")
                                QbzSelect {
                                    menuWidth: 200
                                    options: root.doc.streamingQualities || []
                                    currentIndex: root.doc.streamingQualityIndex || 0
                                    onSelected: function (i) { QbzBridge.settingsSelect("streaming-quality", i) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Limit quality to device")
                                description: QbzBridge.tr("Cap the requested streaming quality at your output device's limit. Applies to local playback only, never to casting.")
                                QbzToggle {
                                    checked: root.doc.limitQualityToDevice === true
                                    onToggled: function (v) { QbzBridge.settingsBool("limit-quality-to-device", v) }
                                }
                            }
                            // POC-NOTE: the read-only "Detected device limit"
                            // row + fallback disclosure (#638 fix 3) are
                            // skipped — the device-cap probe glue is not
                            // ported.

                            SettingsSpacer { }
                            SettingsDivider { }
                            SettingsSpacer { }

                            GroupHeader { text: QbzBridge.tr("OUTPUT") }
                            SettingRow {
                                label: QbzBridge.tr("Audio backend")
                                description: QbzBridge.tr("The audio stack QBZ routes playback through.")
                                QbzSelect {
                                    menuWidth: 220
                                    options: root.doc.backends || []
                                    currentIndex: root.doc.backendIndex || 0
                                    onSelected: function (i) { QbzBridge.settingsSelect("backend", i) }
                                }
                            }
                            // POC-NOTE: the JACK "not bit-perfect"
                            // WarningBanner is skipped (backendIsJack is
                            // published for a future port).
                            SettingRow {
                                label: QbzBridge.tr("Output device")
                                description: QbzBridge.tr("The DAC or sound device that receives audio.")
                                Row {
                                    spacing: 8
                                    // Refresh / release: frees a device QBZ
                                    // holds exclusively and re-enumerates.
                                    Rectangle {
                                        width: 34
                                        height: 34
                                        radius: theme.radiusSm
                                        border.width: 1
                                        border.color: theme.borderSubtle
                                        color: relArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                                        QbzIcon {
                                            name: "refresh-cw"
                                            width: 16
                                            height: 16
                                            anchors.centerIn: parent
                                            tintName: relArea.containsMouse ? "primary" : "muted"
                                        }
                                        MouseArea {
                                            id: relArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: QbzBridge.refreshDevices()
                                        }
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
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("ALSA plugin")
                                description: QbzBridge.tr("How ALSA opens the device — hw is bit-perfect, plughw converts.")
                                QbzSelect {
                                    menuWidth: 220
                                    options: root.doc.alsaPlugins || []
                                    currentIndex: root.doc.alsaPluginIndex || 0
                                    onSelected: function (i) { QbzBridge.settingsSelect("alsa-plugin", i) }
                                }
                            }
                            SettingRow {
                                visible: root.doc.backendIsAlsa === true && root.doc.alsaPluginIsHw === true
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("Hardware volume control")
                                description: QbzBridge.tr("Use the ALSA mixer for volume instead of software gain.")
                                QbzToggle {
                                    checked: root.doc.alsaHardwareVolume === true
                                    onToggled: function (v) { QbzBridge.settingsBool("alsa-hardware-volume", v) }
                                }
                            }
                            SettingRow {
                                visible: root.doc.backendIsAlsa === true
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("DSD playback")
                                description: QbzBridge.tr("How DSD tracks reach the DAC. WARNING: choose DoP or Native only if your DAC supports it — on any other DAC they play as loud noise. Volume is fixed and seeking is disabled in DoP/Native mode. Native additionally needs kernel support for the DAC.")
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

                            GroupHeader { text: QbzBridge.tr("BIT-PERFECT") }
                            SettingRow {
                                label: QbzBridge.tr("Exclusive mode")
                                description: QbzBridge.tr("Lock the device so no other app can resample it.")
                                rowEnabled: root.doc.backendIsAlsa === true
                                QbzToggle {
                                    checked: root.doc.exclusiveMode === true
                                    enabled: root.doc.backendIsAlsa === true
                                    onToggled: function (v) { QbzBridge.settingsBool("exclusive-mode", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Reserve DAC while running")
                                description: QbzBridge.tr("Hold the device reserved so other apps can't grab it.")
                                QbzToggle {
                                    checked: root.doc.reserveDac === true
                                    onToggled: function (v) { QbzBridge.settingsBool("reserve-dac", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("DAC passthrough")
                                description: QbzBridge.tr("Send the bitstream untouched to the DAC.")
                                rowEnabled: root.doc.backendIsPipewire === true
                                QbzToggle {
                                    checked: root.doc.dacPassthrough === true
                                    enabled: root.doc.backendIsPipewire === true
                                    onToggled: function (v) { QbzBridge.settingsBool("dac-passthrough", v) }
                                }
                            }
                            SettingRow {
                                visible: root.doc.dacPassthrough === true && root.doc.backendIsPipewire === true
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("Force bit-perfect")
                                description: QbzBridge.tr("Pin the PipeWire quantum and rate for the active track.")
                                QbzToggle {
                                    checked: root.doc.pwForceBitperfect === true
                                    onToggled: function (v) { QbzBridge.settingsBool("pw-force-bitperfect", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Allow quality fallback")
                                description: QbzBridge.tr("Drop to a lower tier when the requested one is unavailable.")
                                QbzToggle {
                                    checked: root.doc.allowQualityFallback === true
                                    onToggled: function (v) { QbzBridge.settingsBool("allow-quality-fallback", v) }
                                }
                            }
                            // POC-NOTE: the HiFi Wizard row (PipeWire-only
                            // guided DAC setup) is skipped — DacWizardActions
                            // is not ported.

                            SettingsSpacer { }
                            SettingsDivider { }
                            SettingsSpacer { }

                            GroupHeader { text: QbzBridge.tr("STARTUP") }
                            SettingRow {
                                label: QbzBridge.tr("Sync audio settings on startup")
                                description: QbzBridge.tr("Reload saved audio settings into the player when QBZ launches.")
                                QbzToggle {
                                    checked: root.doc.syncAudioOnStartup === true
                                    onToggled: function (v) { QbzBridge.settingsBool("sync-audio-on-startup", v) }
                                }
                            }
                            SettingRow {
                                visible: root.doc.backendIsPipewire === true
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("Lock output device")
                                description: QbzBridge.tr("Keep external routing intact — skip switching the default sink.")
                                rowEnabled: root.doc.dacPassthrough !== true
                                QbzToggle {
                                    checked: root.doc.skipSinkSwitch === true
                                    enabled: root.doc.dacPassthrough !== true
                                    onToggled: function (v) { QbzBridge.settingsBool("skip-sink-switch", v) }
                                }
                            }

                            Item { width: 1; height: 20 }

                            // Reset — restores Audio + Playback defaults.
                            Rectangle {
                                width: Math.max(resetLabel.implicitWidth + 32, 200)
                                height: 36
                                radius: theme.radiusSm
                                border.width: 1
                                border.color: theme.borderSubtle
                                color: resetArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                                Text {
                                    id: resetLabel
                                    anchors.centerIn: parent
                                    text: QbzBridge.tr("Reset to defaults")
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightMedium
                                }
                                MouseArea {
                                    id: resetArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: QbzBridge.settingsReset()
                                }
                            }
                        }

                        // ==================== PLAYBACK ====================
                        Column {
                            visible: root.section === 1
                            width: parent.width
                            spacing: 4

                            GroupHeader { text: QbzBridge.tr("PLAYBACK") }
                            SettingRow {
                                label: QbzBridge.tr("Continue playback after track ends")
                                description: QbzBridge.tr("Keep playing the rest of the album or playlist instead of stopping.")
                                QbzToggle {
                                    checked: root.doc.continuePlayback === true
                                    onToggled: function (v) { QbzBridge.settingsBool("continue-playback", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Show track playing context")
                                description: QbzBridge.tr("Display the context-stack icon in the player.")
                                QbzToggle {
                                    checked: root.doc.showContextIcon === true
                                    onToggled: function (v) { QbzBridge.settingsBool("show-context-icon", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Gapless playback")
                                description: QbzBridge.tr("Play consecutive same-format tracks without a gap.")
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

                            GroupHeader { text: QbzBridge.tr("SESSION") }
                            SettingRow {
                                label: QbzBridge.tr("Restore session on startup")
                                description: QbzBridge.tr("Restore the queue and current track on the next launch.")
                                QbzToggle {
                                    checked: root.doc.persistSession === true
                                    onToggled: function (v) { QbzBridge.settingsBool("persist-session", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Resume playback position")
                                description: QbzBridge.tr("Also seek back to where the saved track left off.")
                                rowEnabled: root.doc.persistSession === true
                                QbzToggle {
                                    checked: root.doc.resumePosition === true
                                    enabled: root.doc.persistSession === true
                                    onToggled: function (v) { QbzBridge.settingsBool("resume-position", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Auto-connect Qobuz Connect on startup")
                                description: QbzBridge.tr("Choose whether Qobuz Connect activates automatically when QBZ launches.")
                                QbzSelect {
                                    menuWidth: 240
                                    options: root.doc.qconnectStartupModes || []
                                    currentIndex: root.doc.qconnectStartupIndex || 0
                                    onSelected: function (i) { QbzBridge.settingsSelect("qconnect-startup", i) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Qobuz Connect device name")
                                description: QbzBridge.tr("The name other Qobuz Connect apps see for this device. Applies on the next connection.")
                                QbzLineEdit {
                                    text: root.doc.qconnectDeviceName || ""
                                    placeholder: root.doc.qconnectDeviceNameDefault || ""
                                    onCommitted: function (s) { QbzBridge.settingsString("qconnect-device-name", s) }
                                }
                            }

                            SettingsSpacer { }
                            SettingsDivider { }
                            SettingsSpacer { }

                            GroupHeader { text: QbzBridge.tr("STREAMING") }
                            SettingRow {
                                label: QbzBridge.tr("Stream uncached tracks")
                                description: QbzBridge.tr("Start uncached tracks via streaming instead of waiting for the full download.")
                                QbzToggle {
                                    checked: root.doc.streamUncached === true
                                    onToggled: function (v) { QbzBridge.settingsBool("stream-uncached", v) }
                                }
                            }
                            SettingRow {
                                visible: root.doc.streamUncached === true
                                height: visible ? (description === "" ? 52 : 64) : 0
                                label: QbzBridge.tr("Initial buffer size")
                                description: QbzBridge.tr("Seconds of audio buffered before streaming playback starts.")
                                Row {
                                    spacing: 12
                                    Item { width: 1; height: 1 }
                                    QbzSlider {
                                        minimum: 1
                                        maximum: 10
                                        value: root.doc.bufferSeconds || 1
                                        anchors.verticalCenter: parent.verticalCenter
                                        onChanged: function (v) { QbzBridge.settingsSlider("buffer-seconds", v) }
                                    }
                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: (root.doc.bufferSeconds || 1) + QbzBridge.tr("s")
                                        color: theme.textSecondary
                                        font.pixelSize: theme.fontBody
                                        font.weight: theme.weightMedium
                                    }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("Streaming only")
                                description: QbzBridge.tr("Skip writing tracks to the local cache while streaming.")
                                QbzToggle {
                                    checked: root.doc.streamingOnly === true
                                    onToggled: function (v) { QbzBridge.settingsBool("streaming-only", v) }
                                }
                            }
                            SettingRow {
                                label: QbzBridge.tr("When quality retries fail")
                                description: QbzBridge.tr("What to do when every quality tier for a track is unavailable.")
                                QbzSelect {
                                    menuWidth: 240
                                    options: root.doc.retryBehaviors || []
                                    currentIndex: root.doc.retryBehaviorIndex || 0
                                    onSelected: function (i) { QbzBridge.settingsSelect("retry-behavior", i) }
                                }
                            }
                        }

                        // ================== APPEARANCE (phase 19) ==========
                        AppearanceSettings {
                            visible: root.section === 2
                            width: parent.width
                            doc: root.doc
                        }

                        // ================== INTEGRATIONS (phase 19) ========
                        IntegrationsSettings {
                            visible: root.section === 6
                            width: parent.width
                            doc: root.doc
                        }

                        // ============ placeholder sections (phase 19) ======
                        // Offline (3) / Local Library (4) / Blacklist (5) /
                        // Developer (7): the subnav rows render the EXACT
                        // section header; the panels land in a later phase
                        // (POC-NOTE).
                        Column {
                            visible: root.section === 3 || root.section === 4
                                || root.section === 5 || root.section === 7
                            width: parent.width
                            spacing: 4
                            GroupHeader {
                                text: root.section === 3 ? QbzBridge.tr("OFFLINE")
                                    : root.section === 4 ? QbzBridge.tr("LOCAL LIBRARY")
                                    : root.section === 5 ? QbzBridge.tr("BLACKLIST")
                                    : QbzBridge.tr("DEVELOPER")
                            }
                            Text {
                                width: parent.width
                                text: QbzBridge.tr("This section is not ported to the Qt frontend yet.")
                                color: theme.textMuted
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                            }
                        }
                    }
                }

                QbzScrollBar {
                    target: flick
                    anchors.right: parent.right
                    anchors.rightMargin: 2
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                }
            }
        }
    }
}
