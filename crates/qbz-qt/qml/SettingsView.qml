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
// - Sub-nav: only Audio + Playback exist (sections 0/1). Appearance,
//   Offline, Local Library, Blacklist, Integrations, Developer, Flatpak/
//   Snap and the "Share logs" entry have no backing glue in the POC.
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
    // Active sub-section: 0 = Audio, 1 = Playback (SettingsState.section).
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

    // --- QbzToggle (primitives/QbzToggle.slint) ---------------------------
    // 40x22 pill r11, 16px knob, accent when on, opacity .4 disabled,
    // 120ms ease-out knob travel. Emits toggled(newValue); never self-flips.
    component QbzToggle: Rectangle {
        property bool checked: false
        property bool enabled: true
        signal toggled(bool value)

        width: 40
        height: 22
        radius: 11
        color: checked ? theme.accent : theme.surfaceElevated
        opacity: enabled ? 1.0 : 0.4

        Rectangle {
            width: 16
            height: 16
            radius: 8
            color: theme.textPrimary
            y: 3
            x: parent.checked ? parent.width - width - 3 : 3
            Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: parent.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (parent.enabled) parent.toggled(!parent.checked)
        }
    }

    // --- QbzSelect (primitives/QbzSelect.slint, standard size) ------------
    // 34px bordered control (elevated, hover -> surface-hover) + a Popup
    // list (surface-main r8 border-muted, 32px rows, capped at 360px with
    // scroll, optional 42px filter box, 24px group-header rows, "BP" badge
    // / speaker glyph on device options). `options` entries are either
    // plain strings or {label, bp, group} objects (the device list).
    component QbzSelect: Rectangle {
        property var options: []
        property int currentIndex: 0
        property int menuWidth: 240
        property int popupWidth: 0
        property bool enabled: true
        property bool searchable: false
        signal selected(int index)

        id: selectRoot
        width: menuWidth
        height: 34
        radius: theme.radiusSm
        border.width: 1
        border.color: theme.borderSubtle
        color: selArea.containsMouse && enabled ? theme.surfaceHover : theme.surfaceElevated
        opacity: enabled ? 1.0 : 0.4

        readonly property int listWidth: Math.max(popupWidth, menuWidth)
        readonly property int rowHeight: 32
        readonly property int headerHeight: 24
        readonly property int searchHeight: searchable ? 42 : 0
        readonly property int maxListHeight: 360
        property string filter: ""

        function optLabel(i) {
            const o = options[i]
            return (typeof o === "string") ? o : (o && o.label !== undefined ? o.label : "")
        }
        function optHasBadges() {
            return options.length > 0 && typeof options[0] !== "string"
        }
        function optBp(i) {
            const o = options[i]
            return o && o.bp === true
        }
        function optGroup(i) {
            const o = options[i]
            return (o && o.group !== undefined) ? o.group : ""
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 10
            spacing: 8
            Text {
                width: parent.width - 16 - 8 - badgeSlot.width - (badgeSlot.visible ? 8 : 0)
                height: parent.height
                text: selectRoot.currentIndex >= 0 && selectRoot.currentIndex < selectRoot.options.length
                    ? selectRoot.optLabel(selectRoot.currentIndex) : ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            // Trailing BP badge / speaker glyph for the CURRENT device option.
            Item {
                id: badgeSlot
                visible: selectRoot.optHasBadges()
                width: visible ? 20 : 0
                height: parent.height
                Text {
                    visible: selectRoot.optHasBadges() && selectRoot.currentIndex < selectRoot.options.length
                        && selectRoot.optBp(selectRoot.currentIndex)
                    anchors.centerIn: parent
                    text: "BP"
                    color: theme.accent
                    font.pixelSize: theme.fontLegal
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 0.5
                }
                QbzIcon {
                    visible: selectRoot.optHasBadges() && selectRoot.currentIndex < selectRoot.options.length
                        && !selectRoot.optBp(selectRoot.currentIndex)
                    name: "volume-2"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: "muted"
                }
            }
            QbzIcon {
                name: "chevron-down"
                width: 16
                height: 16
                anchors.verticalCenter: parent.verticalCenter
                tintName: "muted"
            }
        }
        MouseArea {
            id: selArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: selectRoot.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: {
                if (selectRoot.enabled) {
                    selectRoot.filter = ""
                    popup.open()
                }
            }
        }

        Popup {
            id: popup
            parent: selectRoot
            // Right-anchored: a list wider than the control grows leftward.
            x: selectRoot.width - selectRoot.listWidth
            y: selectRoot.height + 4
            width: selectRoot.listWidth
            height: selectRoot.searchHeight + Math.min(listContent.contentHeight, selectRoot.maxListHeight) + 10
            padding: 0
            closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

            background: Rectangle {
                color: theme.surfaceMain
                radius: theme.radiusSm
                border.width: 1
                border.color: theme.borderMuted
            }
            contentItem: Item {
                implicitWidth: selectRoot.listWidth
                implicitHeight: popup.height

                // Filter box (searchable lists only).
                Rectangle {
                    id: searchBox
                    visible: selectRoot.searchable
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: selectRoot.searchHeight
                    color: "transparent"
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        anchors.bottomMargin: 6
                        spacing: 8
                        QbzIcon {
                            name: "search"
                            width: 14
                            height: 14
                            anchors.verticalCenter: parent.verticalCenter
                            tintName: "muted"
                        }
                        Item {
                            width: parent.width - 14 - 8
                            height: parent.height
                            TextInput {
                                id: searchInput
                                anchors.fill: parent
                                color: theme.textPrimary
                                font.pixelSize: theme.fontBody
                                verticalAlignment: Text.AlignVCenter
                                clip: true
                                text: selectRoot.filter
                                onTextChanged: selectRoot.filter = text
                            }
                            Text {
                                visible: searchInput.text === ""
                                anchors.fill: parent
                                text: QbzBridge.tr("Search…")
                                color: theme.textMuted
                                font.pixelSize: theme.fontBody
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }

                ListView {
                    id: listContent
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: searchBox.bottom
                    anchors.bottom: parent.bottom
                    anchors.topMargin: 5
                    anchors.bottomMargin: 5
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: selectRoot.options

                    delegate: Column {
                        id: optRow
                        required property int index
                        required property var modelData
                        width: listContent.width

                        readonly property string label: selectRoot.optLabel(index)
                        readonly property bool shown: selectRoot.filter === ""
                            || label.toLowerCase().indexOf(selectRoot.filter.toLowerCase()) >= 0

                        // Group-header row (ALSA device sections).
                        Rectangle {
                            visible: optRow.shown && selectRoot.optGroup(optRow.index) !== ""
                            width: parent.width
                            height: visible ? selectRoot.headerHeight : 0
                            color: "transparent"
                            Text {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                anchors.top: parent.top
                                anchors.topMargin: 4
                                height: parent.height - 4
                                text: selectRoot.optGroup(optRow.index)
                                color: theme.textMuted
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightSemibold
                                font.letterSpacing: 0.5
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                        }
                        Rectangle {
                            visible: optRow.shown
                            width: parent.width
                            height: visible ? selectRoot.rowHeight : 0
                            color: optArea.containsMouse ? theme.surfaceHover : "transparent"
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                spacing: 8
                                Text {
                                    width: parent.width - rowBadge.width - (rowBadge.visible ? 8 : 0)
                                    height: parent.height
                                    text: optRow.label
                                    color: optRow.index === selectRoot.currentIndex
                                        ? theme.accent : theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                Item {
                                    id: rowBadge
                                    visible: selectRoot.optHasBadges()
                                    width: visible ? 20 : 0
                                    height: parent.height
                                    Text {
                                        visible: selectRoot.optBp(optRow.index)
                                        anchors.centerIn: parent
                                        text: "BP"
                                        color: theme.accent
                                        font.pixelSize: theme.fontLegal
                                        font.weight: theme.weightSemibold
                                        font.letterSpacing: 0.5
                                    }
                                    QbzIcon {
                                        visible: !selectRoot.optBp(optRow.index)
                                        name: "volume-2"
                                        width: 14
                                        height: 14
                                        anchors.centerIn: parent
                                        tintName: "muted"
                                    }
                                }
                            }
                            MouseArea {
                                id: optArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    popup.close()
                                    selectRoot.selected(optRow.index)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- SettingRow (settings/SettingRow.slint) ---------------------------
    // 52px (64 with a description); label 15 medium + description 12 muted
    // on the left (opacity .45 when disabled), the control flush right.
    component SettingRow: Item {
        property string label: ""
        property string description: ""
        property bool rowEnabled: true
        default property alias control: controlHost.data

        width: parent ? parent.width : 0
        height: description === "" ? 52 : 64

        Column {
            anchors.left: parent.left
            anchors.right: controlHost.left
            anchors.rightMargin: 24
            anchors.verticalCenter: parent.verticalCenter
            spacing: 3
            opacity: rowEnabled ? 1.0 : 0.45
            Text {
                width: parent.width
                text: label
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                visible: description !== ""
                width: parent.width
                text: description
                color: theme.textMuted
                font.pixelSize: 12
                wrapMode: Text.WordWrap
            }
        }
        Item {
            id: controlHost
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: childrenRect.width
            height: childrenRect.height
        }
    }

    // --- GroupHeader / Divider --------------------------------------------
    component GroupHeader: Text {
        color: theme.textMuted
        font.pixelSize: 11
        font.letterSpacing: 1.5
        font.weight: theme.weightSemibold
    }
    component Divider: Rectangle {
        width: parent ? parent.width : 0
        height: 1
        color: theme.borderSubtle
    }
    component Spacer12: Item { width: 1; height: 12 }

    // --- QbzSlider (primitives/QbzSlider.slint) ---------------------------
    // 200x22, 4px r2 track, accent fill, 16px thumb; integer steps. Like the
    // Slint original the thumb follows the pointer during a drag (local
    // dragValue) and commits each step via changed(int).
    component QbzSlider: Item {
        property int minimum: 0
        property int maximum: 10
        property int value: 0
        signal changed(int newValue)

        width: 200
        height: 22

        readonly property int thumbSize: 16
        readonly property real travel: width - thumbSize
        readonly property real fraction: maximum > minimum
            ? Math.max(0, Math.min(1, (value - minimum) / (maximum - minimum))) : 0
        property bool dragging: false
        property real dragFraction: fraction
        readonly property real shownFraction: dragging ? dragFraction : fraction
        onFractionChanged: if (!dragging) dragFraction = fraction

        function commit(frac) {
            const v = Math.round(minimum + Math.max(0, Math.min(1, frac)) * (maximum - minimum))
            if (v !== value) changed(v)
        }

        Rectangle { // track
            x: 0
            y: Math.round((parent.height - height) / 2)
            width: parent.width
            height: 4
            radius: 2
            color: theme.surfaceElevated
        }
        Rectangle { // accent fill
            x: 0
            y: Math.round((parent.height - height) / 2)
            width: parent.thumbSize / 2 + parent.shownFraction * parent.travel
            height: 4
            radius: 2
            color: theme.accent
        }
        Rectangle { // thumb
            width: parent.thumbSize
            height: parent.thumbSize
            radius: parent.thumbSize / 2
            x: parent.shownFraction * parent.travel
            anchors.verticalCenter: parent.verticalCenter
            color: theme.textPrimary
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onPressed: {
                parent.dragging = true
                parent.dragFraction = Math.max(0, Math.min(1, (mouse.x - parent.thumbSize / 2) / parent.travel))
                parent.commit(parent.dragFraction)
            }
            onPositionChanged: {
                if (pressed) {
                    parent.dragFraction = Math.max(0, Math.min(1, (mouse.x - parent.thumbSize / 2) / parent.travel))
                    parent.commit(parent.dragFraction)
                }
            }
            onReleased: parent.dragging = false
        }
    }

    // --- QbzLineEdit (std-widgets LineEdit, settings styling) -------------
    // 240px / 34px elevated r8 bordered input; commits on Enter AND on
    // focus loss (the Tauri onchange semantics — PlaybackSettings.slint).
    component QbzLineEdit: Rectangle {
        property string text: ""
        property string placeholder: ""
        signal committed(string value)

        width: 240
        height: 34
        radius: theme.radiusSm
        border.width: 1
        border.color: theme.borderSubtle
        color: theme.surfaceElevated

        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            color: theme.textPrimary
            font.pixelSize: theme.fontBody
            verticalAlignment: Text.AlignVCenter
            clip: true
            text: parent.text
            onAccepted: parent.committed(text)
            onActiveFocusChanged: if (!activeFocus) parent.committed(text)
            // External republish (e.g. Reset) re-seeds the field while it is
            // not being edited.
            Binding {
                target: input
                property: "text"
                value: input.parent.text
                when: !input.activeFocus
            }
        }
        Text {
            visible: input.text === ""
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            text: placeholder
            color: theme.textMuted
            font.pixelSize: theme.fontBody
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
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
                    // POC-NOTE: Appearance / Offline / Local Library /
                    // Blacklist / Integrations / Developer / Flatpak / Snap /
                    // "Share logs" sub-nav entries are omitted (no backing
                    // glue in the POC).
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

                            Spacer12 { }
                            Divider { }
                            Spacer12 { }

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

                            Spacer12 { }
                            Divider { }
                            Spacer12 { }

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

                            Spacer12 { }
                            Divider { }
                            Spacer12 { }

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

                            Spacer12 { }
                            Divider { }
                            Spacer12 { }

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

                            Spacer12 { }
                            Divider { }
                            Spacer12 { }

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
