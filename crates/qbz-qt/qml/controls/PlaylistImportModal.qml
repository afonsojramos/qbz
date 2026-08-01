// "Import Playlist" — QML port of
// crates/qbz-ui/ui/primitives/PlaylistImportModal.slint (itself a 1:1 port of
// Tauri's PlaylistImportModal.svelte).
//
// Two-step flow: URL entry -> provider auto-detect (RUST-side, one round trip
// per keystroke, exactly as the reference does) -> fetch preview -> rename +
// optional folder -> import with a live progress bar, a status line and an
// append-only log, then an in-panel Summary block. Every interpolated string
// arrives PRE-FORMATTED in the document (playlist_import_qt.rs); this file
// renders them verbatim and never pluralises or interpolates a count itself.
//
// Mounted ONCE in AppShell next to PlaylistPickerModal, NOT inside the sidebar
// (05 §5.8.5): the two surfaces that open it — the sidebar `...` menu and the
// closed-sidebar flyout — have no lifetime guarantee across a navigation, and
// closing the modal must never cancel an in-flight import.
//
// CLOSE SEMANTICS (reference §1.8): the footer Close disables while loading
// (Tauri-verbatim); the header X and the scrim stay clickable mid-import —
// closing only hides the UI, the tokio task runs to completion and still
// toasts, assigns the folder and refreshes the sidebar.
//
// OFFLINE is gated in TWO layers (05 §5.8.7), both required: the sidebar row is
// inert while `QbzSession.offline`, AND this modal carries its own banner and a
// `canFetch` that Rust computes as `provider detected && !offline`. The URL
// field stays enabled — only the fetch is refused.
//
// PROVIDER MARKS. Spotify / Apple Music / Deezer are self-coloured marks and
// render UNTINTED out of qml/assets/brand/, the rule SourceIcon.qml calls
// load-bearing. Tidal's upstream asset is a black `fill` with no colour to
// lose, so it goes through QbzIcon at `textPrimary` instead: the reference
// hardcodes it to white (PlaylistImportModal.slint:271-279) which is
// illegible on the 11 light themes, and a monochrome glyph is exactly what the
// tint table is for. The header's Qobuz mark is the port's existing
// `qobuz-logo-filled.svg` brand asset for the same reason — the reference tints
// its outline logo white, which disappears on a light `surface-card`.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root
    // An unopened overlay is an invisible, non-interactive Item and costs
    // nothing (the AppShell mount comment states the contract).
    visible: doc.open === true

    readonly property var doc: JSON.parse(QbzPlaylistImport.importJson)
    readonly property var logRows: doc.log || []
    readonly property bool loading: doc.loading === true
    readonly property bool showPreview: doc.showPreview === true
    readonly property bool importCompleted: doc.importCompleted === true
    readonly property var folderOptions: doc.folderOptions || []

    // Bumped by every QbzPlaylistImport.open(). The reference remounts the
    // whole Svelte component per open and Slint reproduces it by clearing 14
    // properties; here Rust resets its document and this counter tells the two
    // QML-local text mirrors to drop what the user last typed (05 §5.8.3).
    readonly property int resetSeq: doc.resetSeq || 0
    onResetSeqChanged: {
        urlInput.text = ""
        nameInput.text = ""
    }

    // Absolute qrc prefix for the brand SVGs — same rule as QbzIcon: a relative
    // URL resolves against the CONSUMER's document depth.
    readonly property string brandDir: "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/brand/"

    QbzTheme { id: theme }

    // Scrim — click outside dismisses, EVEN MID-IMPORT (§1.8: closing only
    // hides the UI, the task keeps running).
    Rectangle {
        anchors.fill: parent
        color: "#bf000000"
        MouseArea {
            anchors.fill: parent
            onClicked: QbzPlaylistImport.close()
        }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        // Tauri: 560px max, min-height 420px, max-height 90vh.
        width: Math.min(parent.width - 80, 560)
        // 140 = header 68 + 2 hairlines + footer 70 (16/34/20); the body
        // scrolls inside the Flickable past the 90 % clamp.
        height: Math.min(Math.max(420, bodyCol.implicitHeight + 140), parent.height * 0.9)
        radius: theme.radiusLg
        color: theme.surfaceCard
        border.width: 1
        border.color: theme.borderSubtle
        // Swallow clicks so they never reach the scrim.
        MouseArea { anchors.fill: parent }

        readonly property int contentWidth: card.width - 48

        // ---------------------------- header ----------------------------
        Item {
            id: header
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 68

            Image {
                id: qobuzMark
                anchors.left: parent.left
                anchors.leftMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                width: 26
                height: 26
                source: root.brandDir + "qobuz-logo-filled.svg"
                fillMode: Image.PreserveAspectFit
                sourceSize.width: 52
                sourceSize.height: 52
                smooth: true
                opacity: 0.9
            }
            Text {
                anchors.left: qobuzMark.right
                anchors.leftMargin: 10
                anchors.right: closeX.left
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Import Playlist", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightSemibold
                elide: Text.ElideRight
            }
            // X close — allowed mid-import (§1.8).
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
                    tintName: closeXArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: closeXArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzPlaylistImport.close()
                }
            }
        }
        Rectangle {
            id: headerDiv
            anchors.top: header.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: theme.borderSubtle
        }

        // ---------------------------- footer ----------------------------
        // Declared before the body so the body can anchor to the divider; the
        // footer is pinned to the card bottom either way.
        Item {
            id: footerBar
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: 70

            Row {
                anchors.right: parent.right
                anchors.rightMargin: 24
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 20
                spacing: 12

                SettingsButton {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzSession.tr("Close", QbzSession.trRev)
                    btnHeight: 34
                    minWidth: 0
                    // Tauri-verbatim: the FOOTER close disables while loading;
                    // the header X and the scrim do not.
                    enabled: !root.loading
                    onClicked: QbzPlaylistImport.close()
                }
                // The single accent confirm (ADR-008). Step A fetches, step B
                // imports; it stays visible-but-disabled after completion.
                QbzPrimaryButton {
                    anchors.verticalCenter: parent.verticalCenter
                    btnHeight: 34
                    label: root.showPreview
                        ? (root.loading ? QbzSession.tr("Importing...", QbzSession.trRev)
                                        : QbzSession.tr("Import", QbzSession.trRev))
                        : (root.loading ? QbzSession.tr("Fetching...", QbzSession.trRev)
                                        : QbzSession.tr("Fetch playlist", QbzSession.trRev))
                    btnEnabled: root.showPreview
                        ? (!root.loading && !root.importCompleted)
                        : (root.doc.canFetch === true && !root.loading)
                    onClicked: {
                        if (root.showPreview)
                            QbzPlaylistImport.execute()
                        else
                            QbzPlaylistImport.fetch()
                    }
                }
            }
        }
        Rectangle {
            id: footerDiv
            anchors.bottom: footerBar.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: theme.borderSubtle
        }

        // ----------------------------- body -----------------------------
        // Scrolls when the progress panel outgrows the card.
        Flickable {
            id: bodyFlick
            anchors.top: headerDiv.bottom
            anchors.bottom: footerDiv.top
            anchors.left: parent.left
            anchors.right: parent.right
            clip: true
            contentWidth: width
            contentHeight: bodyCol.implicitHeight
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: bodyCol
                width: card.width
                padding: 24
                spacing: 16

                // --- Offline banner ---------------------------------------
                // The fetch is gated (Rust drives `canFetch` false); the URL
                // input stays enabled, exactly as the reference does.
                //
                // The shared control, with the deltas BlacklistManagerView.qml
                // already records for it: the copy goes in `title` because that
                // is the TONE-coloured line, the glyph is `info` rather than
                // `cloud-off`, and the tone alphas are 10 %/30 % against the
                // reference's #eab3081a / #eab3084d (identical arithmetic, the
                // theme's warning token instead of a literal).
                WarningBanner {
                    width: card.contentWidth
                    visible: QbzSession.offline
                    variant: "warning"
                    title: QbzSession.tr("This feature requires internet connection", QbzSession.trRev)
                }

                // --- Error banner (preview or execute failure) -------------
                WarningBanner {
                    width: card.contentWidth
                    visible: (root.doc.error || "") !== ""
                    variant: "error"
                    title: root.doc.error || ""
                }

                // --- URL input --------------------------------------------
                // Every keystroke routes the provider detection through Rust
                // (the reference does the same because Slint 1.16 strings have
                // no `.contains`; here it is what keeps ONE detector — the
                // crate's `detect_provider_key` — as the source of truth
                // instead of a second JS copy that would drift).
                Column {
                    width: card.contentWidth
                    spacing: 8
                    Text {
                        text: QbzSession.tr("Playlist URL", QbzSession.trRev)
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                        font.weight: theme.weightMedium
                    }
                    // The standard 43px input box (the PlaylistPickerModal
                    // recipe — QbzLineEdit is 34px, commits only on Enter/blur
                    // in its plain arm and has no `enabled` gate, so it cannot
                    // serve a per-keystroke field that must disable mid-fetch).
                    Rectangle {
                        width: parent.width
                        height: 43
                        radius: theme.radiusSm
                        color: theme.surfaceCard
                        border.width: 1
                        border.color: urlInput.activeFocus ? theme.accent : theme.borderSubtle
                        opacity: root.loading ? 0.5 : 1.0

                        TextInput {
                            id: urlInput
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            color: theme.textPrimary
                            font.pixelSize: theme.fontLink
                            verticalAlignment: Text.AlignVCenter
                            clip: true
                            selectByMouse: true
                            enabled: !root.loading
                            onTextEdited: QbzPlaylistImport.urlEdited(text)
                            onAccepted: {
                                if (!root.showPreview && root.doc.canFetch === true && !root.loading)
                                    QbzPlaylistImport.fetch()
                            }
                            // Re-seed from the document while the field is NOT
                            // being edited (the PlaylistPickerModal shape): user
                            // typing breaks a plain `text:` binding for good, so
                            // a reset would never reach the field.
                            Binding {
                                target: urlInput
                                property: "text"
                                value: root.doc.url || ""
                                when: !urlInput.activeFocus
                            }
                        }
                        Text {
                            // Placeholder. Plain string: not localized in the
                            // reference either (it is a URL shape, not copy).
                            visible: urlInput.text === ""
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            verticalAlignment: Text.AlignVCenter
                            text: "https://open.spotify.com/playlist/..."
                            color: theme.textMuted
                            font.pixelSize: theme.fontLink
                            elide: Text.ElideRight
                        }
                    }
                }

                // --- Allowed sources --------------------------------------
                // The detected provider's mark at full opacity, the rest dimmed
                // (the reference's §1.5 homologation: no glow / translate /
                // scale, opacity 0.45 idle / 1.0 active).
                Column {
                    width: card.contentWidth
                    spacing: 8
                    Text {
                        text: QbzSession.tr("ALLOWED SOURCES", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 0.5
                    }
                    Row {
                        spacing: 10
                        Image {
                            width: 70
                            height: 24
                            source: root.brandDir + "spotify-logo.svg"
                            fillMode: Image.PreserveAspectFit
                            sourceSize.width: 140
                            sourceSize.height: 48
                            smooth: true
                            opacity: root.doc.activeProvider === "spotify" ? 1.0 : 0.45
                        }
                        Image {
                            width: 70
                            height: 24
                            source: root.brandDir + "apple-music-logo.svg"
                            fillMode: Image.PreserveAspectFit
                            sourceSize.width: 140
                            sourceSize.height: 48
                            smooth: true
                            opacity: root.doc.activeProvider === "apple" ? 1.0 : 0.45
                        }
                        // Monochrome mark -> the tint table, so it reads on the
                        // dark AND the light themes (see the file header).
                        QbzIcon {
                            width: 70
                            height: 24
                            name: "tidal-tidal"
                            tintName: "textPrimary"
                            opacity: root.doc.activeProvider === "tidal" ? 1.0 : 0.45
                        }
                        Image {
                            width: 70
                            height: 24
                            source: root.brandDir + "deezer-logo.svg"
                            fillMode: Image.PreserveAspectFit
                            sourceSize.width: 140
                            sourceSize.height: 48
                            smooth: true
                            opacity: root.doc.activeProvider === "deezer" ? 1.0 : 0.45
                        }
                    }
                }

                // --- Customization panel (step B) -------------------------
                // Rename + optional folder. Editing the URL reverts to step A
                // (`showPreview` recomputes Rust-side on every keystroke).
                Rectangle {
                    width: card.contentWidth
                    visible: root.showPreview
                    height: customCol.implicitHeight
                    radius: theme.radiusMd
                    color: theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle

                    Column {
                        id: customCol
                        width: parent.width
                        padding: 16
                        spacing: 12

                        Column {
                            width: parent.width - 32
                            spacing: 8
                            Text {
                                text: QbzSession.tr("Playlist name", QbzSession.trRev)
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightMedium
                            }
                            Rectangle {
                                width: parent.width
                                height: 43
                                radius: theme.radiusSm
                                color: theme.surfaceCard
                                border.width: 1
                                border.color: nameInput.activeFocus ? theme.accent : theme.borderSubtle
                                opacity: nameInput.enabled ? 1.0 : 0.5

                                TextInput {
                                    id: nameInput
                                    anchors.fill: parent
                                    anchors.leftMargin: 12
                                    anchors.rightMargin: 12
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontLink
                                    verticalAlignment: Text.AlignVCenter
                                    clip: true
                                    selectByMouse: true
                                    enabled: !root.loading && !root.importCompleted
                                    onTextEdited: QbzPlaylistImport.nameEdited(text)
                                    onAccepted: {
                                        // Enter here executes (step B).
                                        if (!root.loading && !root.importCompleted)
                                            QbzPlaylistImport.execute()
                                    }
                                    // Rust prefills this with the fetched
                                    // playlist name, so the re-seed is not just
                                    // a reset path here — it is how the name
                                    // arrives at all.
                                    Binding {
                                        target: nameInput
                                        property: "text"
                                        value: root.doc.customName || ""
                                        when: !nameInput.activeFocus
                                    }
                                }
                            }
                        }

                        // Folder dropdown — only when the user HAS folders
                        // (index 0 is always "No folder", so length > 1).
                        Column {
                            width: parent.width - 32
                            spacing: 8
                            visible: root.folderOptions.length > 1
                            Text {
                                text: QbzSession.tr("Folder", QbzSession.trRev)
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightMedium
                            }
                            QbzSelect {
                                options: root.folderOptions
                                currentIndex: root.doc.folderIndex || 0
                                menuWidth: parent.width
                                // The reference caps its menu at 380px; here the
                                // control is full-width, so the popup matches it.
                                popupWidth: parent.width
                                searchable: root.folderOptions.length > 8
                                onSelected: function (i) { QbzPlaylistImport.setFolderIndex(i) }
                            }
                        }
                    }
                }

                // --- Progress panel ---------------------------------------
                // Spinner header, determinate bar, status + current-track
                // lines, the append-only log, and the Summary block.
                Rectangle {
                    width: card.contentWidth
                    visible: root.doc.progressVisible === true
                    height: progressCol.implicitHeight
                    radius: theme.radiusMd
                    color: theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle

                    Column {
                        id: progressCol
                        width: parent.width
                        padding: 16
                        spacing: 10

                        Item {
                            width: parent.width - 32
                            height: 20
                            Text {
                                anchors.left: parent.left
                                anchors.right: spinnerSlot.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzSession.tr("Conversion progress", QbzSession.trRev)
                                color: theme.textPrimary
                                font.pixelSize: theme.fontBody
                                font.weight: theme.weightMedium
                                elide: Text.ElideRight
                            }
                            // Tauri: a 14px accent ring while loading.
                            Item {
                                id: spinnerSlot
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 14
                                height: 14
                                QbzSpinner {
                                    anchors.centerIn: parent
                                    size: 14
                                    visible: root.loading
                                    spinning: root.loading
                                }
                            }
                        }

                        Column {
                            width: parent.width - 32
                            spacing: 6
                            visible: root.doc.hasProgress === true

                            // Determinate 6px bar (the LocalLibrarySettings
                            // recipe); the track uses the subtle border tone
                            // because the panel is already surface-elevated.
                            Rectangle {
                                width: parent.width
                                height: 6
                                radius: 3
                                color: theme.borderSubtle
                                clip: true
                                Rectangle {
                                    width: parent.width * Math.max(0, Math.min(1, root.doc.progress || 0))
                                    height: parent.height
                                    radius: 3
                                    color: theme.accent
                                }
                            }
                            // Status line — the 12px Tauri metric, fixed height
                            // (the CJK floor femtovg needed; harmless here and
                            // it keeps the panel from reflowing per event).
                            Text {
                                width: parent.width
                                height: 18
                                text: root.doc.statusLine || ""
                                color: theme.textSecondary
                                font.pixelSize: 12
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                            // Current-track line — "Artist - Title" while
                            // matching, "Part i/n" while adding.
                            Text {
                                width: parent.width
                                visible: (root.doc.currentTrack || "") !== ""
                                height: visible ? 16 : 0
                                text: root.doc.currentTrack || ""
                                color: theme.textMuted
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                        }

                        // Append-only log. The height is static arithmetic over
                        // the ROW COUNT, never row geometry (the reference calls
                        // that out as a recursion footgun). No autoscroll —
                        // Tauri parity. Render-only rows: no MouseAreas here.
                        Flickable {
                            width: parent.width - 32
                            visible: root.logRows.length > 0
                            height: visible ? Math.min(root.logRows.length * 20, 160) : 0
                            contentWidth: width
                            contentHeight: root.logRows.length * 20
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds

                            Column {
                                width: parent.width
                                Repeater {
                                    model: root.logRows
                                    delegate: Text {
                                        required property var modelData
                                        width: parent ? parent.width : 0
                                        height: 20
                                        text: modelData.message || ""
                                        // Hex literals kept — the reference
                                        // hardcodes them (Tauri did too), and
                                        // the port has no success/danger glyph
                                        // ramp for a 13px log row.
                                        color: modelData.status === "success" ? "#34d399"
                                             : (modelData.status === "error" ? "#f87171" : theme.textMuted)
                                        font.pixelSize: theme.fontLegal
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }

                        // Summary block — pre-formatted lines, hairline
                        // separated, "" = hidden.
                        Column {
                            width: parent.width - 32
                            spacing: 6
                            visible: (root.doc.summaryPlaylist || "") !== ""

                            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
                            Text {
                                text: QbzSession.tr("Summary", QbzSession.trRev)
                                color: theme.textPrimary
                                font.pixelSize: theme.fontBody
                                font.weight: theme.weightSemibold
                            }
                            Text {
                                width: parent.width
                                height: 18
                                text: root.doc.summaryPlaylist || ""
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                height: 18
                                text: root.doc.summaryMatched || ""
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                height: 18
                                text: root.doc.summarySkipped || ""
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                visible: (root.doc.summaryParts || "") !== ""
                                height: visible ? 18 : 0
                                text: root.doc.summaryParts || ""
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }

        // The 14px gutter scrollbar, inset 4px from the right — the port's
        // shared control, attached as a SIBLING of the Flickable (it is an
        // Item with a `target`, not a QtQuick.Controls ScrollBar).
        QbzScrollBar {
            target: bodyFlick
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.top: bodyFlick.top
            anchors.bottom: bodyFlick.bottom
        }
    }
}
