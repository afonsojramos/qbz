// Settings — the QML port of crates/qbz-ui/ui/settings/SettingsView.slint:
// a 92px title header, a 232px left sub-navigation, and the active panel in
// a touch-draggable Flickable with a ListScrollbar replica.
//
// This file is the COMPOSITION ROOT only. Every section is its own file in
// this directory, 1:1 with the Slint's own per-section .slint files:
//
//   0 Audio          -> AudioSettings.qml
//   1 Playback       -> PlaybackSettings.qml
//   2 Appearance     -> AppearanceSettings.qml
//   3 Offline        -> OfflineSettings.qml
//   4 Local Library  -> LocalLibrarySettings.qml (+ PlexSettings.qml)
//   5 Blacklist      -> BlacklistSettings.qml
//   6 Integrations   -> IntegrationsSettings.qml
//   7 Developer      -> DeveloperSettings.qml
//   8 Flatpak/Snap   -> SandboxSettings.qml (only on a sandboxed install)
//
// All state is ONE JSON document (QbzBridge.settingsJson, settings_qt.rs
// SettingsDoc). Controls never keep local truth: they call the
// settingsBool/Select/Slider/String invokables, Rust persists + applies +
// republishes, and the rows re-render from the new document (the Slint
// "single source of truth in SettingsState" pattern).
//
// The NavButtons row inside the 92px header is a 0px placeholder — nav
// history lives in the global HeaderBar in this port, like every other view.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // The parsed settingsJson document ({} until the first publish lands).
    property var doc: ({})
    // Active sub-section (display order — see the header comment).
    //
    // A BINDING onto bridge state, never local truth. The content Loader
    // unmounts this whole view when the user navigates away, so a plain
    // `property int section: 0` was lost on every round trip — the Blacklist
    // panel's "Manage" chevron opens the manager VIEW, and coming Back landed
    // on Audio. See `bridge.rs`'s note on `settings_section`.
    //
    // Consequently every sub-nav row calls `QbzBridge.settingsSetSection(n)`
    // and NOTHING assigns `root.section` directly: in QML an imperative
    // assignment destroys the binding it lands on, so a single surviving
    // `root.section = n` would work once and then re-introduce the bug one
    // navigation later, in a way that looks fixed under casual testing.
    readonly property int section: QbzBridge.settingsSection

    readonly property bool sandboxed: {
        const m = (doc.dev || ({})).installMethod || ""
        return m === "flatpak" || m === "snap"
    }

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
                text: QbzSession.tr("Settings", QbzSession.trRev)
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
                id: subNav
                width: 232
                height: parent.height

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
                            // Theme tokens on both arms — the row sits on
                            // surface-elevated / surface-hover, so the active
                            // glyph is "textPrimary" and NOT the fixed-white
                            // "primary" bake (white-on-white on light themes).
                            tintName: parent.parent.active ? "textPrimary" : "secondary"
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

                Column {
                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: 24
                    anchors.rightMargin: 12
                    anchors.topMargin: 4
                    spacing: 4

                    SubNavItem {
                        name: "volume-2"
                        label: QbzSession.tr("Audio", QbzSession.trRev)
                        active: root.section === 0
                        onClicked: QbzBridge.settingsSetSection(0)
                    }
                    SubNavItem {
                        name: "play-fill"
                        label: QbzSession.tr("Playback", QbzSession.trRev)
                        active: root.section === 1
                        onClicked: QbzBridge.settingsSetSection(1)
                    }
                    SubNavItem {
                        name: "layers"
                        label: QbzSession.tr("Appearance", QbzSession.trRev)
                        active: root.section === 2
                        onClicked: QbzBridge.settingsSetSection(2)
                    }
                    SubNavItem {
                        name: "cloud-download"
                        label: QbzSession.tr("Offline", QbzSession.trRev)
                        active: root.section === 3
                        onClicked: QbzBridge.settingsSetSection(3)
                    }
                    SubNavItem {
                        name: "hard-drive"
                        label: QbzSession.tr("Local Library", QbzSession.trRev)
                        active: root.section === 4
                        onClicked: QbzBridge.settingsSetSection(4)
                    }
                    SubNavItem {
                        name: "blind-eye"
                        label: QbzSession.tr("Blacklist", QbzSession.trRev)
                        active: root.section === 5
                        onClicked: QbzBridge.settingsSetSection(5)
                    }
                    SubNavItem {
                        name: "refresh-cw"
                        label: QbzSession.tr("Integrations", QbzSession.trRev)
                        active: root.section === 6
                        onClicked: QbzBridge.settingsSetSection(6)
                    }
                    SubNavItem {
                        name: "bug"
                        label: QbzSession.tr("Developer", QbzSession.trRev)
                        active: root.section === 7
                        onClicked: QbzBridge.settingsSetSection(7)
                    }
                    // Sandboxed installs only (Flatpak / Snap permissions).
                    SubNavItem {
                        visible: root.sandboxed
                        name: "info"
                        label: (root.doc.dev || ({})).installMethod === "snap"
                            ? QbzSession.tr("Snap", QbzSession.trRev)
                            : QbzSession.tr("Flatpak", QbzSession.trRev)
                        active: root.section === 8
                        onClicked: QbzBridge.settingsSetSection(8)
                    }
                }

                // Always-visible log affordance, pinned to the bottom of the
                // sub-nav (people filing issues could not find it under
                // Developer). Opens the on-disk log — see DeveloperSettings.
                Column {
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: 24
                    anchors.rightMargin: 12
                    anchors.bottomMargin: 16
                    SubNavItem {
                        name: "cloud-upload"
                        label: QbzSession.tr("Share logs", QbzSession.trRev)
                        active: false
                        // Opens the VIEWER (filter / search / copy / bundle /
                        // upload), 1:1 with the reference. It used to hand the
                        // raw file to the desktop's file manager, which is
                        // what "Open log file" in Developer still does.
                        onClicked: QbzShell.logOpen()
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

                        AudioSettings {
                            visible: root.section === 0
                            width: parent.width
                            doc: root.doc
                        }
                        PlaybackSettings {
                            visible: root.section === 1
                            width: parent.width
                            doc: root.doc
                        }
                        AppearanceSettings {
                            visible: root.section === 2
                            width: parent.width
                            doc: root.doc
                        }
                        OfflineSettings {
                            visible: root.section === 3
                            width: parent.width
                            doc: root.doc
                        }
                        LocalLibrarySettings {
                            visible: root.section === 4
                            width: parent.width
                            doc: root.doc
                            confirmHost: confirmHost
                        }
                        BlacklistSettings {
                            visible: root.section === 5
                            width: parent.width
                            doc: root.doc
                        }
                        IntegrationsSettings {
                            visible: root.section === 6
                            width: parent.width
                            doc: root.doc
                        }
                        DeveloperSettings {
                            visible: root.section === 7
                            width: parent.width
                            doc: root.doc
                        }
                        SandboxSettings {
                            visible: root.section === 8 && root.sandboxed
                            width: parent.width
                            doc: root.doc
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

    // Danger-zone confirmations for the whole view — declared LAST so
    // declaration order alone puts it over every panel and the sub-nav.
    // Panels that need it are handed the reference (see LocalLibrarySettings
    // / PlexSettings `confirmHost`).
    SettingsConfirmHost { id: confirmHost }

    // The per-folder settings modal. Same reasoning as the confirm host: it
    // must overlay the whole view, so it cannot live inside the scrolled
    // panel that opens it. The reference mounts its counterpart at the
    // AppShell root for exactly the same reason (LibFolderEditModal.slint:5-8).
    LibFolderEditModal { doc: root.doc }
}
