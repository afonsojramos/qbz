// QbzSelect (primitives/QbzSelect.slint, standard size) — extracted from
// SettingsView.qml in phase 19 so every settings panel shares the one
// replica (the ui-control-alignment-standard searchable dropdown: the
// filter box shows past ~8 options via `searchable: true`).
// 34px bordered control (elevated, hover -> surface-hover) + a Popup
// list (surface-main r8 border-muted, 32px rows, capped at 360px with
// scroll, optional 42px filter box, 24px group-header rows, "BP" badge
// / speaker glyph on device options). `options` entries are either
// plain strings or {label, bp, group} objects (the device list).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Rectangle {
    property var options: []
    property int currentIndex: 0
    property int menuWidth: 240
    property int popupWidth: 0
    property bool enabled: true
    property bool searchable: false
    // Bootstrap-style small variant (QbzSelect.slint:74-76). The default is the
    // standard Settings size; TOOLBARS opt in with `sm: true` — 30px tall, r6,
    // NO outline (it matches the favorites toolbar buttons), 12px label and a
    // 14px chevron, and 40px narrower because toolbar selects were needlessly
    // wide (QbzSelect.slint:86-88,102-105,118-119,126,147-148,299).
    property bool sm: false
    signal selected(int index)

    QbzTheme { id: theme }

    id: selectRoot
    width: sm ? Math.max(0, menuWidth - 40) : menuWidth
    height: sm ? 30 : 34
    radius: sm ? 6 : theme.radiusSm
    border.width: sm ? 0 : 1
    border.color: theme.borderSubtle
    // Resting fill goes translucent under the dynamic background so the field
    // shows through the control (QbzSelect.slint:110-116). Hover keeps its own
    // token, which is already translucent.
    color: selArea.containsMouse && enabled
        ? theme.surfaceHover
        : (theme.ambientOn ? theme.surfaceElevatedA50 : theme.surfaceElevated)
    opacity: enabled ? 1.0 : 0.4

    // `width`, NOT `menuWidth`: QbzSelect.slint:89-91 resolves the open list as
    // `max(popup-width, EFF-WIDTH)`, and eff-width is `menu-width - 40px` at
    // `sm` (:88) — the same value it uses for the collapsed control. Reading the
    // RAW menuWidth here made every toolbar dropdown's PANEL 40px wider than the
    // reference's (MyQBZ sort 180 vs 140, kind filter 200 vs 160, detail sort
    // 170 vs 130, and the Local Library toolbar / LabelReleases selects with it).
    // The three `popupWidth` callers (AudioSettings 480, CastPicker 260,
    // LyricsControlsFlyout 178) are all non-`sm`, where width === menuWidth and
    // popupWidth wins anyway, so none of them moves.
    readonly property int listWidth: Math.max(popupWidth, selectRoot.width)
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
            && options[0] && options[0].bp !== undefined
    }
    function optBp(i) {
        const o = options[i]
        return o && o.bp === true
    }
    function optGroup(i) {
        const o = options[i]
        return (o && o.group !== undefined) ? o.group : ""
    }
    function optDetail(i) {
        const o = options[i]
        return (o && o.detail !== undefined) ? o.detail : ""
    }
    function openPopup() {
        if (!selectRoot.enabled || selectRoot.options.length === 0)
            return
        selectRoot.filter = ""
        popup.open()
        if (selectRoot.searchable)
            searchInput.forceActiveFocus()
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: selectRoot.sm ? 10 : 12
        anchors.rightMargin: selectRoot.sm ? 8 : 10
        spacing: 8
        Text {
            width: parent.width - 16 - 8 - badgeSlot.width - (badgeSlot.visible ? 8 : 0)
            height: parent.height
            text: selectRoot.currentIndex >= 0 && selectRoot.currentIndex < selectRoot.options.length
                ? selectRoot.optLabel(selectRoot.currentIndex) : ""
            color: theme.textPrimary
            font.pixelSize: selectRoot.sm ? 12 : theme.fontBody
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
            width: selectRoot.sm ? 14 : 16
            height: selectRoot.sm ? 14 : 16
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
                selectRoot.openPopup()
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
                            text: QbzSession.tr("Search…", QbzSession.trRev)
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
                        height: visible
                            ? (selectRoot.optDetail(optRow.index) !== "" ? 50 : selectRoot.rowHeight)
                            : 0
                        color: optArea.containsMouse ? theme.surfaceHover : "transparent"
                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 8
                            Column {
                                width: parent.width - rowBadge.width - (rowBadge.visible ? 8 : 0)
                                height: parent.height
                                anchors.verticalCenter: parent.verticalCenter
                                Text {
                                    width: parent.width
                                    height: selectRoot.optDetail(optRow.index) !== "" ? 26 : parent.height
                                    text: optRow.label
                                    color: optRow.index === selectRoot.currentIndex
                                        ? theme.accent : theme.textSecondary
                                    font.pixelSize: selectRoot.sm ? 12 : theme.fontBody
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                Text {
                                    visible: selectRoot.optDetail(optRow.index) !== ""
                                    width: parent.width
                                    height: visible ? 18 : 0
                                    text: selectRoot.optDetail(optRow.index)
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLegal
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
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
