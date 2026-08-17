// "Find available version" — the replacement modal for a track Qobuz PULLED
// from the catalogue (2026-08-17 unavailable-tracks contract §6).
//
// It has NO Slint counterpart: the feature never existed there, and the only
// prior art is the Tauri TrackReplacementModal.svelte, which showed Qobuz's raw
// relevance order with no scoring at all. The list here is RANKED by the shared
// weighted matcher (qbz-playlist-import), the head of it is preselected, and an
// ISRC-identical candidate is labelled as the exact relink it is — see
// track_replace_qt.rs for the ordering rules. The human still confirms; nothing
// swaps itself.
//
// Mounted ONCE in AppShell next to PlaylistPickerModal, for the same reason:
// the apply RELOADS the playlist underneath the modal, so a modal parented into
// the playlist view would be torn down by its own success.
//
// Data: QbzTrackReplace.replaceJson (track_replace_qt.rs ReplaceDoc). The
// candidate list is built in Rust — ranking, the ISRC short-circuit, the
// same-id guard and the streamable filter all live there, so this file renders
// rows and owns nothing but the query field's local text.
//
// No `z`: this file's neighbours in AppShell are ordered by DECLARATION, and
// ADR-009's z >= 3000 governs IN-PANE modals. A window-level overlay covering
// the content pane's rounded corners is correct — a window-wide scrim covers
// them too.

// No QtQuick.Controls import: every type here is QtQuick or a project
// component, and nothing on this surface uses the attached ToolTip that pulls
// the module into rows/TrackRow.qml.
import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root
    // An unopened overlay is an invisible, non-interactive Item and costs
    // nothing (the AppShell mount comment states the contract).
    visible: doc.open === true

    readonly property var doc: JSON.parse(QbzTrackReplace.replaceJson)
    readonly property var rows: doc.candidates || []
    readonly property bool loading: doc.loading === true
    readonly property bool applying: doc.applying === true
    readonly property string selectedId: doc.selectedId || ""

    // The ONLY local state in the file: what the user has typed but not yet
    // submitted. Everything else is Rust's document, so a republish (a late
    // search landing, the apply latch) can never fight the view.
    property string queryText: ""

    onVisibleChanged: if (visible) queryText = root.doc.query || ""

    // Re-seed when RUST changes the query (the open path builds "title artist")
    // and the field is not being edited.
    Connections {
        target: QbzTrackReplace
        function onReplaceJsonChanged() {
            if (!queryInput.activeFocus)
                root.queryText = root.doc.query || ""
        }
    }

    function submitQuery() {
        if (root.applying)
            return
        var q = root.queryText.trim()
        if (q !== "")
            QbzTrackReplace.search(q)
    }

    QbzTheme { id: theme }

    // Scrim — click outside dismisses, unless a write is in flight: closing
    // mid-apply would hide the outcome of a playlist mutation the user cannot
    // undo from anywhere else.
    Rectangle {
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: if (!root.applying) QbzTrackReplace.close()
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(parent.width - 80, 620)
        height: Math.min(Math.max(420, panel.implicitHeight + 66), parent.height * 0.9)
        radius: theme.radiusLg
        color: theme.surfaceCard
        border.width: 1
        border.color: theme.borderSubtle
        clip: true

        // Swallow clicks so they never reach the scrim.
        MouseArea { anchors.fill: parent }

        Column {
            id: panel
            width: parent.width

            // --- Header ---------------------------------------------------
            Item {
                width: parent.width
                height: 72
                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: 24
                    anchors.right: closeX.left
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3
                    Text {
                        width: parent.width
                        text: QbzSession.tr("Find available version", QbzSession.trRev)
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightSemibold
                        elide: Text.ElideRight
                    }
                    // WHICH row is being repaired. The modal is opened from a
                    // row context menu and the list below is full of near-
                    // identical titles, so naming the dead track is not
                    // decoration — it is how the user checks they right-clicked
                    // the row they meant.
                    Text {
                        width: parent.width
                        text: (root.doc.deadTitle || "")
                              + ((root.doc.deadArtist || "") !== ""
                                  ? " — " + root.doc.deadArtist : "")
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        elide: Text.ElideRight
                    }
                }
                Item {
                    id: closeX
                    anchors.right: parent.right
                    anchors.rightMargin: 24
                    anchors.verticalCenter: parent.verticalCenter
                    width: 28
                    height: 28
                    QbzIcon {
                        anchors.centerIn: parent
                        name: "x"
                        width: 17
                        height: 17
                        tintName: closeArea.containsMouse ? "textPrimary" : "muted"
                    }
                    MouseArea {
                        id: closeArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        enabled: !root.applying
                        onClicked: QbzTrackReplace.close()
                    }
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }

            // --- Query --------------------------------------------------
            // Editable and re-searchable: "title artist" is only a first guess,
            // and the case this feature exists for is precisely the one where
            // the recording moved and the words changed with it.
            Item {
                width: parent.width
                height: 43 + 24
                Rectangle {
                    anchors.left: parent.left
                    anchors.leftMargin: 24
                    anchors.right: parent.right
                    anchors.rightMargin: 24
                    anchors.verticalCenter: parent.verticalCenter
                    height: 43
                    radius: theme.radiusSm
                    color: theme.surfaceCard
                    border.width: 1
                    border.color: queryInput.activeFocus ? theme.accent : theme.borderSubtle

                    TextInput {
                        id: queryInput
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 40
                        color: theme.textPrimary
                        font.pixelSize: theme.fontLink
                        verticalAlignment: Text.AlignVCenter
                        clip: true
                        selectByMouse: true
                        enabled: !root.applying
                        text: root.queryText
                        onTextEdited: root.queryText = text
                        onAccepted: root.submitQuery()
                        // Re-seed while not being edited (the
                        // PlaylistPickerModal Binding shape).
                        Binding {
                            target: queryInput
                            property: "text"
                            value: root.queryText
                            when: !queryInput.activeFocus
                        }
                    }
                    Item {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: 22
                        height: 22
                        QbzIcon {
                            anchors.centerIn: parent
                            name: "search"
                            width: 15
                            height: 15
                            tintName: searchArea.containsMouse ? "textPrimary" : "muted"
                        }
                        MouseArea {
                            id: searchArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            enabled: !root.applying
                            onClicked: root.submitQuery()
                        }
                    }
                }
            }

            // --- Candidates ---------------------------------------------
            Item {
                width: parent.width
                // Fills whatever the card has left between the query row and
                // the footer, so the list scrolls instead of the card growing.
                height: Math.max(120, card.height - 72 - 1 - 67 - 66)

                QbzSpinner {
                    anchors.centerIn: parent
                    size: 30
                    visible: root.loading
                }

                // Nothing came back. It is a real outcome, not an error: the
                // recording may simply not be on Qobuz any more in any form.
                Text {
                    anchors.centerIn: parent
                    width: parent.width - 96
                    visible: !root.loading && root.rows.length === 0
                    text: QbzSession.tr("No available version found", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLink
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                }

                ListView {
                    id: list
                    anchors.fill: parent
                    anchors.leftMargin: 24
                    anchors.rightMargin: 24
                    anchors.topMargin: 4
                    anchors.bottomMargin: 4
                    visible: !root.loading && root.rows.length > 0
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: root.rows
                    spacing: 2

                    delegate: Rectangle {
                        id: cand
                        required property var modelData
                        readonly property bool picked: root.selectedId === modelData.id
                        width: ListView.view ? ListView.view.width : 0
                        height: 56
                        radius: theme.radiusSm
                        color: picked ? theme.surfaceElevated
                             : (candArea.containsMouse ? theme.surfaceHover : "transparent")
                        border.width: picked ? 1 : 0
                        border.color: theme.accent

                        Row {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 10

                            // Cover. `artPath` arrives EMPTY on the first
                            // publish and is filled by a second one once the
                            // download settles, so the placeholder is the
                            // normal first frame, not a failure.
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 40
                                height: 40
                                radius: 3
                                color: theme.surfaceElevated
                                clip: true
                                Image {
                                    anchors.fill: parent
                                    visible: (cand.modelData.artPath || "") !== ""
                                    source: cand.modelData.artPath || ""
                                    sourceSize.width: 80
                                    sourceSize.height: 80
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                }
                            }

                            Column {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 40 - 10 - rightCell.width - 10
                                spacing: 2
                                Row {
                                    width: parent.width
                                    spacing: 6
                                    Text {
                                        width: Math.min(implicitWidth,
                                            parent.width - (exactBadge.visible ? exactBadge.width + 6 : 0)
                                                         - (weakBadge.visible ? weakBadge.width + 6 : 0))
                                        text: cand.modelData.title || ""
                                        color: theme.textPrimary
                                        font.pixelSize: theme.fontLink
                                        elide: Text.ElideRight
                                    }
                                    // THE EXACT RELINK. Same ISRC = the same
                                    // recording under a new catalog id, which
                                    // is the owner's "a veces cambia el ID del
                                    // álbum" case. Saying so is the difference
                                    // between a certainty and a good guess, and
                                    // the user is the one signing off.
                                    Rectangle {
                                        id: exactBadge
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: cand.modelData.exact === true
                                        width: exactText.implicitWidth + 12
                                        height: 16
                                        radius: 3
                                        color: theme.accent
                                        Text {
                                            id: exactText
                                            anchors.centerIn: parent
                                            text: QbzSession.tr("Exact match", QbzSession.trRev)
                                            color: "#ffffff"
                                            font.pixelSize: 9
                                            font.weight: theme.weightMedium
                                        }
                                    }
                                    // Below the matcher's own confidence floor.
                                    // Offered anyway — a human is confirming,
                                    // and a weak candidate is often the only
                                    // one there is — but never dressed up as a
                                    // match.
                                    Rectangle {
                                        id: weakBadge
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: cand.modelData.weak === true
                                        width: weakText.implicitWidth + 12
                                        height: 16
                                        radius: 3
                                        color: theme.surfaceElevated
                                        border.width: 1
                                        border.color: theme.borderSubtle
                                        Text {
                                            id: weakText
                                            anchors.centerIn: parent
                                            text: QbzSession.tr("Low confidence", QbzSession.trRev)
                                            color: theme.textMuted
                                            font.pixelSize: 9
                                            font.weight: theme.weightMedium
                                        }
                                    }
                                }
                                Text {
                                    width: parent.width
                                    text: (cand.modelData.artist || "")
                                          + ((cand.modelData.album || "") !== ""
                                              ? "  ·  " + cand.modelData.album : "")
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLegal
                                    elide: Text.ElideRight
                                }
                            }

                            Row {
                                id: rightCell
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 8
                                // Quality is a DECISION AID here, not chrome:
                                // the owner's other case is a tier being pulled
                                // and another added, so which tier a candidate
                                // carries is often the whole choice.
                                QualityBadge {
                                    anchors.verticalCenter: parent.verticalCenter
                                    tierOverride: cand.modelData.qualityTier || ""
                                    label: cand.modelData.qualityDetail || ""
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: cand.modelData.duration || ""
                                    color: theme.textMuted
                                    font.pixelSize: theme.fontLegal
                                }
                            }
                        }

                        MouseArea {
                            id: candArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            enabled: !root.applying
                            onClicked: QbzTrackReplace.select(String(cand.modelData.id))
                        }
                    }
                }
            }
        }

        // --- Footer, pinned to the card bottom ---------------------------
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: footerRow.top
            anchors.bottomMargin: 16
            height: 1
            color: theme.borderSubtle
        }
        // What the swap will DO to the playlist, said before it is done. The
        // reference silently appended; this one takes the dead row's slot, and
        // when the reposition call fails the toast says so instead.
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 24
            anchors.right: footerRow.left
            anchors.rightMargin: 12
            anchors.verticalCenter: footerRow.verticalCenter
            visible: root.rows.length > 0
            text: QbzSession.tr("The replacement takes the unavailable track's place in the playlist.",
                                QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
        }
        Row {
            id: footerRow
            anchors.right: parent.right
            anchors.rightMargin: 24
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 16
            spacing: 12

            SettingsButton {
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Cancel", QbzSession.trRev)
                btnHeight: 34
                minWidth: 0
                enabled: !root.applying
                onClicked: QbzTrackReplace.close()
            }
            // The single accent confirm (ADR-008). Disabled until something is
            // selected — there is no in-body error state to fall back on.
            QbzPrimaryButton {
                anchors.verticalCenter: parent.verticalCenter
                btnHeight: 34
                label: root.applying
                    ? QbzSession.tr("Replacing...", QbzSession.trRev)
                    : QbzSession.tr("Replace", QbzSession.trRev)
                btnEnabled: !root.applying && !root.loading && root.selectedId !== ""
                onClicked: QbzTrackReplace.apply()
            }
        }
    }
}
