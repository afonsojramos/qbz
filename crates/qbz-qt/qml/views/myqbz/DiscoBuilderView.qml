// Discography Builder — the full-page flow that assembles an artist's complete
// discography into a saved `kind='artist_collection'` collection. QML port of
// myqbz/DiscographyBuilderView.slint:418-787 (spec 01 §8).
//
// Route "discobuilder". Entry: the artist page's overflow -> "Create Artist
// Collection". Exit: the footer Cancel — there is NO per-view back button
// (:438: the shell owns the global < > nav).
//
// TWO-PART SHELL (:8-11, :422-428, :727-736): everything scrolls except a
// FIXED 60px footer pinned at the bottom as a sibling of the scroller (NOT
// sticky-inside-scroll), so the last rows never peek past it.
//
// WINDOWING (spec 01 §8.9): a discography is tens-to-hundreds of groups and
// every row mounts a QualityBadgeFull, so the table is ONE recycled ListView
// over the FLATTENED row descriptors Rust publishes as `doc.rows`
// (`{kind, groupKey, groupIsCompilation, restOpacity, candidate}`), with
// `cacheBuffer: 44*10`. NOT a ListView of groups each holding a Repeater of
// alternates: that shape leaves `rows` unread and makes every delegate a
// variable-height Column. Everything above the table — the artist header,
// the NAME field, the ORDER BY row, the 20px spacer, the loading/error/empty
// band and the table's own rule + column header — is the ListView.header, so
// ONE scroller drives the page; the content's 24px bottom padding is the
// ListView.footer. In the three non-populated states the model is empty and
// the header carries the whole page: the ListView is never branched out of the
// tree, because a new `model` assignment resets contentY (recipe §8).
//
// The root is `color: "transparent"` VERBATIM (:419) — not the ambient ternary
// the other views use, and therefore no `radius: 12` either: there is no fill
// of its own to round, the content frame's surfaceMain shows through, and the
// footer's surfaceMain matches it (AppShell's bezel nubs own the corners).

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"
import "../local"

Rectangle {
    id: root
    color: "transparent"

    QbzTheme { id: theme }

    readonly property var doc: parseDoc()
    function parseDoc() {
        try {
            return JSON.parse(QbzDisco.discoJson)
        } catch (e) {
            return ({})
        }
    }
    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // --- Document slices (spec 02 §5.8 DiscoDoc) --------------------------
    readonly property string artistId: doc.artistId || ""
    readonly property bool loading: doc.loading === true
    readonly property string loadError: doc.loadError || ""
    readonly property var groups: doc.groups || []
    // The flattened table (spec 01 §8.9) — one entry per RENDERED row, in the
    // same order as `groups`, each carrying its precomputed `restOpacity`.
    readonly property var rows: doc.rows || []
    readonly property string orderBy: doc.orderBy || "release_date"
    readonly property int selectedCount: doc.selectedCount || 0
    readonly property int groupCount: doc.groupCount || 0
    readonly property bool creating: doc.creating === true
    readonly property string defaultName: doc.defaultName || ""
    // Precedence exactly as the Slint writes it (:599,610,621-623,634-636):
    // loading -> loadError -> empty -> the table.
    readonly property bool populated: !root.loading && root.loadError === ""
        && root.groups.length > 0

    // --- Collection name (QML-local) --------------------------------------
    // The bridge (spec 02 §4.3) has no `setName`: `create(name)` takes it, so
    // the field's truth lives here and Rust seeds it once through
    // `defaultName` ("{artist} — Complete Discography").
    // DIVERGENCE from spec 01 §8.7, forced by that surface: Slint cannot trim
    // and exposes `name-trimmed-empty` from Rust; with one implementation
    // there is nothing to diverge from, so the trim is done here.
    property string collectionName: ""
    property bool nameEdited: false
    readonly property bool nameTrimmedEmpty: root.collectionName.trim() === ""
    // A new artist re-seeds; a late-arriving default (the name resolves after
    // the fetch) must not clobber what the user typed.
    onArtistIdChanged: {
        root.nameEdited = false
        root.collectionName = root.defaultName
    }
    onDefaultNameChanged: if (!root.nameEdited) root.collectionName = root.defaultName
    // Slint's `reset` blanks `collection-name` before EVERY fetch
    // (myqbz_builder.rs:690) and `install` re-seeds it, so a name the user typed
    // during a previous open never survives into the next one. Without this the
    // `nameEdited` latch keeps a stale edit forever when the builder is
    // re-opened for the SAME artist (`artistId` and `defaultName` both unchanged,
    // so neither handler above fires).
    onLoadingChanged: if (root.loading) {
        root.nameEdited = false
        root.collectionName = ""
        // A fresh load is a fresh table: without this the preserved offset
        // (below) survives into the NEXT artist and `restoreScroll` opens his
        // discography already scrolled to the previous one's row. Slint's
        // Flickable lands at the top because `reset` empties the viewport.
        root.keepY = 0
    }
    Component.onCompleted: if (!root.nameEdited && root.collectionName === "")
        root.collectionName = root.defaultName

    // Footer count. Rust pre-renders the ONE true plural in this domain with
    // `qbz_i18n::tf` and publishes it as `footerCount` (spec 01 §4/§8.7) —
    // QML has no plural entry point. The fallback below only runs if that key
    // is absent, and then falls through to the English source (an untranslated
    // msgid renders, it does not break — recipe §7).
    readonly property string footerText: {
        var pre = root.doc.footerCount || ""
        if (pre !== "")
            return pre
        var src = root.groupCount === 1
            ? "{} of {} album selected" : "{} of {} albums selected"
        return root.t(src).replace("{}", String(root.selectedCount))
                          .replace("{}", String(root.groupCount))
    }

    /// The six column labels (:677-700) — uppercase literals for the same
    /// reason as the eyebrow, `letterSpacing 1.2`, and the per-cell alignment
    /// the Slint gives each one. `titleW` is the 1fr ALBUM cell.
    function headCells(rev, titleW) {
        return [
            { "w": 56, "label": QbzSession.tr("YEAR", rev), "center": false },
            { "w": 72, "label": QbzSession.tr("TYPE", rev), "center": false },
            { "w": titleW, "label": QbzSession.tr("ALBUM", rev), "center": false },
            { "w": 72, "label": QbzSession.tr("TRACKS", rev), "center": true },
            { "w": 60, "label": QbzSession.tr("SOURCE", rev), "center": true },
            { "w": 156, "label": QbzSession.tr("QUALITY", rev), "center": true }
        ]
    }

    /// ORDER BY segments (:552-555). Built by a function that TAKES `rev` so
    /// the array rebuilds on a live language switch.
    function orderSegments(rev) {
        return [
            { "id": "release_date", "label": QbzSession.tr("Release date", rev) },
            { "id": "title", "label": QbzSession.tr("Title", rev) },
            { "id": "manual", "label": QbzSession.tr("Manual", rev) }
        ]
    }

    // --- Scroll preservation ----------------------------------------------
    // Every mutation (a checkbox, an override, a re-sort) republishes the whole
    // DiscoDoc, so `groups` is a NEW array and QQuickItemView::setModel()
    // resets contentY to 0 — one checkbox click would throw the user from row
    // 40 to the top. Slint's Flickable keeps its viewport-y across the same
    // republish, so restoring it IS the parity behaviour.
    property real keepY: 0
    onRowsChanged: Qt.callLater(root.restoreScroll)
    function restoreScroll() {
        if (root.keepY <= 0 || rowList.contentY > 0)
            return
        rowList.contentY = Math.min(
            root.keepY, Math.max(0, rowList.contentHeight - rowList.height))
    }

    // --- Everything above the table (the ListView header) ------------------
    Component {
        id: pageHead

        Column {
            width: rowList.width
            spacing: 0

            // content padding-top (:433).
            Item { width: 1; height: 24 }

            // ── header.builder-header (:442-486) ──
            Row {
                width: parent.width
                spacing: 20
                // 72px circle. The Slint draws NO placeholder glyph — an
                // artist without a portrait is the bare surface (:446-458).
                Rectangle {
                    width: 72
                    height: 72
                    radius: 36
                    color: theme.surfaceElevated
                    clip: true
                    RoundedImage {
                        anchors.fill: parent
                        visible: (root.doc.artistAvatarUrl || "") !== ""
                        source: root.doc.artistAvatarPath || ""
                        radius: 36
                    }
                }
                Column {
                    width: Math.max(0, parent.width - 72 - 20)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    // Uppercase in the msgid: Slint has no text-transform and
                    // writes the literal in caps (:465-471), and the eight
                    // catalogues carry it that way.
                    Text {
                        text: root.t("BUILD ARTIST COLLECTION")
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 1.5
                    }
                    Text {
                        width: parent.width
                        text: (root.doc.artistName || "") === ""
                            ? "—" : root.doc.artistName
                        color: theme.textPrimary
                        font.pixelSize: 28
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                }
            }
            Item { width: 1; height: 24 }

            // ── label.field — the collection-name input (:489-524) ──
            Column {
                width: parent.width
                spacing: 8
                Text {
                    text: root.t("NAME")
                    color: theme.textMuted
                    font.pixelSize: 10
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 1.5
                }
                // Inlined rather than QbzLineEdit's plain arm: that arm is
                // 240x34 / borderSubtle and this field is a one-off 540x40
                // bordered `surfaceElevated` (spec 01 §8.3). Do not fork the
                // shared control for it.
                Rectangle {
                    width: 540
                    height: 40
                    color: theme.surfaceCard
                    radius: 8
                    border.width: 1
                    border.color: nameField.activeFocus
                        ? theme.accent : theme.surfaceElevated
                    TextInput {
                        id: nameField
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        color: theme.textPrimary
                        font.pixelSize: 14
                        verticalAlignment: Text.AlignVCenter
                        clip: true
                        selectByMouse: true
                        text: root.collectionName
                        onTextEdited: {
                            root.nameEdited = true
                            root.collectionName = text
                        }
                        // Re-seed from Rust while the field is NOT being
                        // edited (the QbzLineEdit.qml:174-179 idiom).
                        Binding {
                            target: nameField
                            property: "text"
                            value: root.collectionName
                            when: !nameField.activeFocus
                        }
                    }
                }
            }
            Item { width: 1; height: 16 }

            // ── .order-row — the segmented control (:527-594) ──
            Row {
                width: parent.width
                spacing: 12
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.t("ORDER BY")
                    color: theme.textMuted
                    font.pixelSize: 10
                    font.weight: theme.weightSemibold
                    font.letterSpacing: 1.5
                }
                // A THIRD toggle shape (32px tall, labelled, bordered box) —
                // neither QbzSegToggle (icon segments) nor QbzToolButton, and
                // one call site, so it stays inline (spec 01 §8.4).
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: segRow.implicitWidth
                    height: 32
                    radius: 8
                    border.width: 1
                    border.color: theme.surfaceElevated
                    clip: true
                    Row {
                        id: segRow
                        height: parent.height
                        spacing: 0
                        Repeater {
                            model: root.orderSegments(QbzSession.trRev)
                            delegate: Rectangle {
                                id: seg
                                required property var modelData
                                required property int index
                                readonly property bool active:
                                    root.orderBy === seg.modelData.id
                                // padding-l/r 14 (:572-573).
                                width: segLabel.implicitWidth + 28
                                height: parent.height
                                // NOT accent (ADR-008, :558-563): a surface
                                // highlight, told apart from hover by tone +
                                // the bolder label.
                                color: seg.active
                                    ? theme.surfaceHover
                                    : (segArea.containsMouse
                                        ? theme.surfaceElevated : "transparent")
                                Rectangle {
                                    visible: seg.index > 0
                                    x: 0
                                    width: 1
                                    height: parent.height
                                    color: theme.surfaceElevated
                                }
                                Text {
                                    id: segLabel
                                    anchors.centerIn: parent
                                    text: seg.modelData.label
                                    color: (seg.active || segArea.containsMouse)
                                        ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: 12
                                    font.weight: seg.active
                                        ? theme.weightSemibold : theme.weightMedium
                                }
                                MouseArea {
                                    id: segArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: QbzDisco.setOrder(seg.modelData.id)
                                }
                            }
                        }
                    }
                }
            }
            Item { width: 1; height: 20 }

            // ── loading / error / empty — one 96px band (:597-632) ──
            Item {
                width: parent.width
                visible: !root.populated
                height: visible ? 96 : 0
                Text {
                    anchors.fill: parent
                    text: root.loading
                        ? root.t("Loading...")
                        : (root.loadError !== ""
                            ? root.loadError : root.t("No results found"))
                    // The Slint uses Theme.favorite here, not Theme.danger
                    // (:614) — keep it.
                    color: (!root.loading && root.loadError !== "")
                        ? theme.favorite : theme.textMuted
                    font.pixelSize: 14
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }

            // ── .groups table preamble: border-top + 4px pad (:639-640) ──
            Rectangle {
                width: parent.width
                visible: root.populated
                height: visible ? 1 : 0
                color: theme.surfaceElevated
            }
            Item { width: 1; visible: root.populated; height: visible ? 4 : 0 }

            // ── header row (.row.header-row, :643-702) ──
            // The 7-column template is inlined here and in DiscoCandidateRow —
            // a MATCHED PAIR. 28/56/72/1fr/72/60/156, padding 8, spacing 10.
            Item {
                id: colHead
                width: parent.width
                visible: root.populated
                height: visible ? 36 : 0
                readonly property real titleW: Math.max(
                    0, colHead.width - 16 - 444 - 60)

                Rectangle {
                    y: parent.height - 1
                    width: parent.width
                    height: 1
                    color: theme.surfaceElevated
                }
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 10

                    // [0] 28px — tri-state select-all. The indeterminate state
                    // renders minus.svg inside the same accent disc (the
                    // SelectCheck/TreeRow idiom), never a hand-drawn dash.
                    Item {
                        width: 28
                        height: parent.height
                        SelectCheck {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            on: root.doc.allChecked === true
                            partial: root.doc.allChecked !== true
                                && root.doc.someChecked === true
                            onToggled: QbzDisco.toggleAll()
                        }
                    }
                    // [1..6] — the column labels, in order.
                    Repeater {
                        model: root.headCells(QbzSession.trRev, colHead.titleW)
                        delegate: Text {
                            required property var modelData
                            width: modelData.w
                            height: parent.height
                            text: modelData.label
                            color: theme.textMuted
                            font.pixelSize: 10
                            font.weight: theme.weightSemibold
                            font.letterSpacing: 1.2
                            horizontalAlignment: modelData.center
                                ? Text.AlignHCenter : Text.AlignLeft
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }
                    }
                }
            }
        }
    }

    // content padding-bottom (:436).
    Component {
        id: bottomPad
        Item { width: 1; height: 24 }
    }

    // --- The scroller (:424-428) -----------------------------------------
    ListView {
        id: rowList
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: footerBar.top
        // content padding-l/r 32 (:431-432); the scrollbar gutter sits inside
        // the right padding, so it never overlaps a row.
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        // spec 01 §8.9: 44 * 10.
        cacheBuffer: 440
        model: root.populated ? root.rows : []
        header: pageHead
        footer: bottomPad
        onContentYChanged: if (contentY > 0) root.keepY = contentY

        // ONE row — a group's primary, or one of its alternates (:705-722).
        // Every field comes off the Rust row descriptor: `restOpacity` is
        // precomputed there (primary = isCompilation ? 0.65 : 1.0, alternate =
        // 0.72 unconditionally), so it is never re-derived here.
        delegate: DiscoCandidateRow {
            required property var modelData
            width: rowList.width
            candidate: modelData.candidate || ({})
            groupKey: modelData.groupKey || ""
            alt: modelData.kind === "alt"
            groupIsCompilation: modelData.groupIsCompilation === true
            restOpacity: modelData.restOpacity !== undefined
                ? modelData.restOpacity : 1.0
        }
    }

    QbzScrollBar {
        target: rowList
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: rowList.top
        anchors.bottom: rowList.bottom
    }

    // --- footer.builder-footer (:728-785) --------------------------------
    Rectangle {
        id: footerBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 60
        color: theme.surfaceMain

        Rectangle {
            y: 0
            width: parent.width
            height: 1
            color: theme.surfaceElevated
        }
        Text {
            anchors.left: parent.left
            anchors.leftMargin: 32
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, parent.width - 32 - 88 - actionRow.width - 16)
            text: root.footerText
            color: theme.textSecondary
            font.pixelSize: 13
            elide: Text.ElideRight
        }
        Row {
            id: actionRow
            // Asymmetric right padding (:739) — 88 clears the back-to-top FAB.
            anchors.right: parent.right
            anchors.rightMargin: 88
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            SettingsButton {
                text: root.t("Cancel")
                btnHeight: 36
                minWidth: 0
                onClicked: QbzDisco.back()
            }
            QbzPrimaryButton {
                label: root.creating
                    ? root.t("Loading...") : root.t("Create Collection")
                btnHeight: 36
                btnEnabled: !(root.creating || root.selectedCount === 0
                    || root.nameTrimmedEmpty)
                onClicked: QbzDisco.create(root.collectionName)
            }
        }
    }
}
