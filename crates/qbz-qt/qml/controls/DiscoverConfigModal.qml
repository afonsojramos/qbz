// DiscoverConfigModal — QML port of
// crates/qbz-ui/ui/discover/DiscoverConfigModal.slint (the per-tab
// "Customize" modal opened by the Discover toolbar gear): reorder +
// show/hide the active tab's rails, with a perf-warning banner, an
// enabled/total count and a Reset-to-defaults footer.
//
// No Save button — every toggle / move / reset persists live to
// discover_prefs.db and re-renders the three tabs from the cached section
// data (src/discover_config_qt.rs), so the list stays authoritative.
//
// Mount as the LAST child of the view root (declaration order IS z-order)
// with `anchors.fill: parent`; ADR-009's ">= 3000" is satisfied structurally
// AND by the explicit z below.
//
// POC-NOTE: the Slint modal's Recommendations arm (explainer + cache window
// + "Refresh now") drives ExternalRecoState, and the external reco engine is
// not ported — the toolbar gear is DISABLED on that tab instead (HomeView).

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Item {
    id: root

    // "home" | "editorPicks" | "forYou" — the tab whose sections are edited.
    property string tab: "home"
    readonly property bool opened: root._open
    property bool _open: false

    QbzTheme { id: theme }

    readonly property var doc: {
        try {
            return JSON.parse(QbzBridge.discoverConfigJson)
        } catch (e) {
            return {}
        }
    }
    // Only render rows once Rust has published THIS tab's payload.
    readonly property bool mine: (doc.tab || "") === root.tab
    readonly property var rows: root.mine ? (doc.rows || []) : []

    function open(forTab) {
        root.tab = forTab
        QbzBridge.discoverConfigOpen(forTab)
        root._open = true
    }
    function close() {
        root._open = false
    }

    visible: root._open
    enabled: root._open
    z: 3100

    // Scrim — declared FIRST so the panel below it in source order (and thus
    // above it in z) keeps its own presses.
    Rectangle {
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: root.close()
        }
    }

    Rectangle {
        id: panel
        width: Math.min(root.width - 80, 520)
        height: Math.min(panelCol.implicitHeight + 40, root.height * 0.78)
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        radius: theme.radiusMd
        color: theme.surfaceCard
        border.width: 1
        border.color: theme.borderSubtle
        clip: true

        // Swallow clicks so they never reach the scrim.
        MouseArea { anchors.fill: parent }

        Column {
            // NOT `body`: WarningBanner below has a `body` PROPERTY, and an
            // id that shadows a child's property name is a trap waiting for
            // the next reader.
            id: panelCol
            x: 20
            y: 20
            width: parent.width - 40
            spacing: 14

            // --- Title + close --------------------------------------------
            Item {
                width: parent.width
                height: 28
                Text {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("Customize", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                }
                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 28
                    height: 28
                    radius: 6
                    color: cfgCloseArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        anchors.centerIn: parent
                        name: "x"
                        width: 17
                        height: 17
                        tintName: cfgCloseArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: cfgCloseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.close()
                    }
                }
            }

            // --- Perf-warning banner (the shared WarningBanner replica) ----
            WarningBanner {
                width: parent.width
                variant: "warning"
                body: QbzSession.tr("Enabling more sections increases load time.", QbzSession.trRev)
            }

            // --- Count line -----------------------------------------------
            Text {
                width: parent.width
                text: QbzSession.tr("{} of {} enabled", QbzSession.trRev)
                        .replace("{}", root.mine ? (doc.enabled || 0) : 0)
                        .replace("{}", root.mine ? (doc.total || 0) : 0)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightMedium
            }

            // --- Section list ---------------------------------------------
            // Explicit, content-derived height capped to what is left of the
            // panel; the Flickable scrolls past that.
            Item {
                width: parent.width
                height: Math.max(0, Math.min(listCol.height, root.height * 0.78 - 260))
                clip: true
                Flickable {
                    id: listFlick
                    anchors.fill: parent
                    contentWidth: width
                    contentHeight: listCol.height
                    boundsBehavior: Flickable.StopAtBounds
                    Column {
                        id: listCol
                        width: listFlick.width
                        spacing: 2
                        Repeater {
                            model: root.rows
                            delegate: Rectangle {
                                id: cfgRow
                                required property var modelData
                                required property int index
                                width: listCol.width
                                height: 40
                                radius: theme.radiusSm
                                color: cfgRowArea.containsMouse ? theme.surfaceHover : "transparent"

                                // Whole-row tap toggles (primary affordance).
                                // Declared FIRST so the checkbox and the two
                                // reorder buttons below sit ON TOP of it and
                                // keep their own presses.
                                MouseArea {
                                    id: cfgRowArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: QbzBridge.discoverToggleSection(root.tab, cfgRow.modelData.id)
                                }

                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 10
                                    anchors.rightMargin: 6
                                    spacing: 12

                                    QbzCheckbox {
                                        anchors.verticalCenter: parent.verticalCenter
                                        checked: cfgRow.modelData.enabled
                                        onToggled: QbzBridge.discoverToggleSection(root.tab, cfgRow.modelData.id)
                                    }
                                    Text {
                                        width: parent.width - 18 - 28 - 28 - 3 * 12
                                        height: parent.height
                                        text: cfgRow.modelData.label
                                        color: cfgRow.modelData.enabled ? theme.textPrimary : theme.textMuted
                                        font.pixelSize: theme.fontBody
                                        font.weight: theme.weightMedium
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }
                                    ReorderButton {
                                        anchors.verticalCenter: parent.verticalCenter
                                        glyph: "chevron-up"
                                        buttonEnabled: cfgRow.index > 0
                                        onClicked: QbzBridge.discoverMoveSection(root.tab, cfgRow.modelData.id, -1)
                                    }
                                    ReorderButton {
                                        anchors.verticalCenter: parent.verticalCenter
                                        glyph: "chevron-down"
                                        buttonEnabled: cfgRow.index < root.rows.length - 1
                                        onClicked: QbzBridge.discoverMoveSection(root.tab, cfgRow.modelData.id, 1)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Empty state — a tab whose sections have not been fetched yet.
            Text {
                visible: root.rows.length === 0
                width: parent.width
                height: 34
                text: QbzSession.tr("No sections to configure yet.", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                verticalAlignment: Text.AlignVCenter
            }

            // --- Footer: Reset to defaults --------------------------------
            Rectangle {
                width: resetRow.width + 24
                height: 34
                radius: theme.radiusSm
                color: resetArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                border.width: 1
                border.color: theme.borderSubtle
                Row {
                    id: resetRow
                    anchors.centerIn: parent
                    spacing: 8
                    QbzIcon {
                        name: "rotate-ccw"
                        width: 15
                        height: 15
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "secondary"
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: QbzSession.tr("Reset to defaults", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody
                    }
                }
                MouseArea {
                    id: resetArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.discoverResetTab(root.tab)
                }
            }
        }
    }

    // 28x28 ghost reorder button (the Slint ReorderButton): dims and drops
    // the pointer cursor at the list boundary.
    component ReorderButton: Rectangle {
        id: reorderRoot
        property string glyph: "chevron-up"
        property bool buttonEnabled: true
        signal clicked()

        width: 28
        height: 28
        radius: theme.radiusSm
        opacity: reorderRoot.buttonEnabled ? 1.0 : 0.3
        color: (reorderRoot.buttonEnabled && reorderArea.containsMouse)
            ? theme.surfaceHover : "transparent"

        QbzIcon {
            anchors.centerIn: parent
            name: reorderRoot.glyph
            width: 16
            height: 16
            tintName: "secondary"
        }
        MouseArea {
            id: reorderArea
            anchors.fill: parent
            enabled: reorderRoot.buttonEnabled
            hoverEnabled: reorderRoot.buttonEnabled
            cursorShape: reorderRoot.buttonEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: reorderRoot.clicked()
        }
    }
}
