// Right-side Queue panel — QML port of
// crates/qbz-ui/ui/shell/QueueSidebar.slint.
//
// Header with the two tabs ("Queue" / "History"), the NOW PLAYING card
// area, the UP NEXT list region, and the EMPTY states (the queue is empty
// until phase 4 — QbzBridge.queueModel is an empty QVariantList).
//
// POC-NOTE: the footer (count line + clear / save-as-playlist / infinite /
// sleep-timer actions + inline queue search) is omitted — in the Slint
// panel it only mounts when the queue is non-empty, which is never until
// phase 4. Drag-reorder, pagination, the now-playing favorite toggle and
// the sleep-timer popover are likewise phase-4 scope.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    color: "transparent"
    clip: true

    // Queue (0) / History (1) — Slint QueueState.tab.
    property int tab: 0
    readonly property bool queueEmpty: QbzBridge.queueModel.length === 0

    QbzTheme { id: theme }

    // One tab — plain text, no pill (QBZ design rule): active brightest,
    // inactive dim, brighten on hover.
    component QueueTab: Text {
        property bool active: false
        signal clicked()

        font.pixelSize: theme.fontBody
        font.weight: theme.weightSemibold
        color: active ? theme.textPrimary
             : tabArea.containsMouse ? theme.textSecondary : theme.textMuted
        verticalAlignment: Text.AlignVCenter
        height: parent ? parent.height : 0

        MouseArea {
            id: tabArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // --- Header: tabs + close button -------------------------------
        Item {
            width: parent.width
            height: 44

            Row {
                anchors.left: parent.left
                anchors.leftMargin: theme.spacingMd
                anchors.right: closeBtn.left
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                height: 28
                spacing: 14

                QueueTab {
                    text: QbzBridge.tr("Queue")
                    active: root.tab === 0
                    onClicked: root.tab = 0
                }
                Text {
                    text: "|"
                    color: theme.textDisabled
                    font.pixelSize: theme.fontBody
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                }
                QueueTab {
                    text: QbzBridge.tr("History")
                    active: root.tab === 1
                    onClicked: root.tab = 1
                }
            }
            // Close button (mirrors the LyricsSidebar X).
            Rectangle {
                id: closeBtn
                anchors.right: parent.right
                anchors.rightMargin: theme.spacingSm
                anchors.verticalCenter: parent.verticalCenter
                width: 28
                height: 28
                radius: theme.radiusSm
                color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "x"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: closeArea.containsMouse ? "primary" : "secondary"
                }
                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.toggleQueue()
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // --- Body --------------------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 45

            // ===== Queue tab =====
            Column {
                visible: root.tab === 0
                anchors.fill: parent
                anchors.margins: 10
                spacing: 12

                // NOW PLAYING section — mounts only with a current track.
                Text {
                    visible: !root.queueEmpty
                    text: QbzBridge.tr("NOW PLAYING")
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 0.5
                }
                // UP NEXT section — mounts only with upcoming tracks.
                Text {
                    visible: !root.queueEmpty
                    text: QbzBridge.tr("UP NEXT")
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 0.5
                }
                Repeater {
                    model: QbzBridge.queueModel
                    // Phase 4 renders real rows (44px QueueRow); unreachable
                    // while the model is empty.
                    delegate: Item {}
                }

                // Empty state — no current track and no upcoming.
                Column {
                    visible: root.queueEmpty
                    width: parent.width
                    topPadding: 48 - 10
                    spacing: 6
                    Text {
                        width: parent.width
                        text: QbzBridge.tr("Your queue is empty")
                        color: theme.textSecondary
                        font.pixelSize: theme.fontBody
                        font.weight: theme.weightMedium
                        horizontalAlignment: Text.AlignHCenter
                    }
                    Text {
                        width: parent.width
                        text: QbzBridge.tr("Play an album or track to get started")
                        color: theme.textMuted
                        font.pixelSize: 12
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                    }
                }
            }

            // ===== History tab =====
            Column {
                visible: root.tab === 1
                anchors.fill: parent
                anchors.margins: 10
                spacing: 8

                Text {
                    text: QbzBridge.tr("RECENTLY PLAYED")
                    color: theme.textMuted
                    font.pixelSize: 11
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 0.5
                }
                // POC-NOTE: history is always empty until phase 4.
                Text {
                    width: parent.width
                    topPadding: 48 - 10
                    text: QbzBridge.tr("Nothing played yet")
                    color: theme.textMuted
                    font.pixelSize: theme.fontBody
                    horizontalAlignment: Text.AlignHCenter
                }
            }
        }
    }
}
