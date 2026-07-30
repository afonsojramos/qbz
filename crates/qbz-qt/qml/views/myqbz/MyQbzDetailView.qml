// MyQbzDetailView — the My QBZ Mixtape / Collection DETAIL route
// ("mixtapedetail"), the port of myqbz/MixtapeDetailView.slint:596-1272.
// Hero + sticky toolbar + body (list / grid / expanded) + the bulk bar.
// ONE document: `QbzMyQbz.detailJson` (spec 02 §5.2 `DetailDoc`).
//
// No per-view back button — the shell owns the global < > nav (.slint :613).
//
// --- PAGE FRAME, and the ONE structural deviation ------------------------
// The .slint puts hero + toolbar + body inside a single Flickable
// (:631-660: padding 16 / 32 / 100 / 32, spacing 0, top-aligned), so its hero
// scrolls away. Here the hero + toolbar are a FIXED header and only the body
// scrolls, which is the port's established shape for a WINDOWED body
// (`views/LibraryView.qml:1269`, `views/local/LocalTracksTab.qml:135`).
//
// Why, since 1:1 is the bar: an artist_collection holds 200+ items and every
// row carries a QualityBadgeFull, so the body MUST be a recycling
// ListView/GridView (spec 01 §15.1, non-negotiable). A recycling view cannot
// be content-sized inside an outer Flickable, and `ListView.header` is not
// available to the GRID arm either — the grid is hosted in a clipped Item and
// given `width: parent.width + 20 - 24` to reproduce Slint's column count
// (spec 01 §7.4), which would offset and over-widen a header inside it. The
// paddings, the 32px hero→body gap and the 10px toolbar tail are all kept;
// the toolbar simply became literally sticky, which is what §7.2 calls it.
//
// --- THE MODEL IS PATCHED, NEVER REPLACED, AND PARSED ONCE --------------
// `QbzMyQbz` carries no signals: every Rust-side change arrives as a whole new
// `detailJson`. `refresh()` is the single entry point: ONE `JSON.parse` per
// republish (spec 01 §15.2), feeding both `doc` and `syncRows` from the same
// object so the two can never be a republish out of step.
// A naive `model: JSON.parse(...).items` would push a NEW
// array into the view on every `resolve_items` badge, every `ensure_expanded`
// batch and every checkbox — and `QQuickItemView::setModel()` resets contentY
// to 0 (and can SIGSEGV from inside its own teardown; the analysis is at
// `views/LibraryView.qml:174-221`). So `syncRows()` keeps the array identity
// whenever the DERIVATION did not change (same collection, same positions in
// the same order) and patches the row objects in place, bumping `patchRev` —
// the notifier a plain JS object mutation does not have. The delegates read
// `patchRev` (see MyQbzDetailRow.qml's header). The array is replaced only on
// a real re-derive (sort / filter / search / another collection), which is
// exactly when Slint also rebuilds and the scroll reset is correct.
//
// Reused, not rebuilt: rows/TrackRow.qml (inline tracks),
// controls/QualityBadgeFull, controls/CardMenu, controls/QbzContextMenu,
// controls/QbzSegToggle, controls/QbzMultiSelectBar, controls/QbzToolButton,
// controls/QbzLoadingDots, controls/QbzLineEdit, controls/QbzSelect,
// controls/QbzCircleAction, cards/CollectionMosaic, views/local/SelectCheck,
// theme/QbzScrollBar, theme/QbzSpinner.
//
// `controls/QbzLineEdit.qml` is NOT 1:1 with `primitives/ExpandableSearch.slint`
// — five pre-existing, app-wide divergences (leading glyph 13 vs 14 at `sm`,
// input + placeholder 12px vs 15, row spacing 7 vs 8, focus border `accent`
// vs `focus-ring`, X hover `surfaceHover` vs `surface-elevated`). Shipped as
// they are: fixing them changes the Local Library toolbars too (spec 01 §3.2).

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz
import "../../cards"
import "../../controls"
import "../../rows"
import "../../theme"
import "../local"

Rectangle {
    id: root

    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    function trs(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // ---------------------- file-private components ----------------------

    /// One 10px column label of the 8-column header (.slint :1123-1129).
    component ColHead: Text {
        color: theme.textMuted
        font.pixelSize: 10
        font.weight: theme.weightSemibold
        font.letterSpacing: 1.2
        height: 34
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    /// A filter-popup section header (.slint :968-978): 20px, 10px semibold,
    /// letter-spacing 0.8, left-aligned.
    component SectionHead: Text {
        width: parent ? parent.width : 0
        height: 20
        color: theme.textMuted
        font.pixelSize: 10
        font.weight: theme.weightSemibold
        font.letterSpacing: 0.8
        verticalAlignment: Text.AlignVCenter
    }

    /// One filter row (.slint `MenuOption`, :47-77): 30px, r4, hover
    /// `surface-hover`, padding 8, spacing 8, a leading `SelectCheck` and a
    /// 13px label that goes accent when selected. The WHOLE row is clickable,
    /// not just the box.
    component MenuOption: Rectangle {
        id: mo
        property string label: ""
        property bool selected: false
        signal chosen()

        width: parent ? parent.width : 0
        height: 30
        radius: 4
        color: moArea.containsMouse ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 8
            SelectCheck {
                anchors.verticalCenter: parent.verticalCenter
                on: mo.selected
                onToggled: mo.chosen()
            }
            Text {
                width: Math.max(0, parent.width - 21)
                height: parent.height
                text: mo.label
                color: mo.selected ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontLegal
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: moArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: mo.chosen()
        }
    }

    // ------------------------------ document ------------------------------
    function parseDoc() {
        try {
            return JSON.parse(QbzMyQbz.detailJson)
        } catch (e) {
            return ({})
        }
    }
    /// ONE parse per republish (spec 01 §15.2). `refresh()` is the only writer
    /// and the only caller of `syncRows`, so `doc` and `rows` can never be a
    /// republish out of step — the earlier shape had a `readonly` binding here
    /// AND a second `parseDoc()` inside the change handler, i.e. two parses of
    /// a document that in expanded mode carries every item's inline track list.
    /// The initial value is still a BINDING so the very first frame is already
    /// correct (a plain `({})` would flash the not-found state for a frame);
    /// the first `refresh()` assignment replaces it.
    property var doc: root.parseDoc()

    readonly property bool loading: doc.loading === true
    readonly property bool found: doc.found === true
    readonly property string kind: doc.kind || ""
    readonly property int itemCount: doc.itemCount || 0
    readonly property string viewMode: doc.viewMode || "list"
    readonly property bool selectMode: doc.selectMode === true
    readonly property int selectedCount: doc.selectedCount || 0
    readonly property int filterCount: doc.filterCount || 0
    readonly property bool hasAnyFilter: doc.hasAnyFilter === true

    // --------------------------- the row model -----------------------------
    property var rows: []
    property string rowsKey: ""
    property int patchRev: 0

    /// Identity of the current derivation: the collection plus the visible
    /// positions in order. `position` is the stable persisted key (spec 02
    /// §5.2), so this changes exactly when Slint would rebuild its model.
    function rowsSignature(d) {
        var it = d.items || []
        var s = (d.id || "") + "#" + it.length
        for (var i = 0; i < it.length; i++) s += "," + it[i].position
        return s
    }
    function syncRows(d) {
        var fresh = d.items || []
        var sig = root.rowsSignature(d)
        if (sig === root.rowsKey && root.rows.length === fresh.length) {
            for (var i = 0; i < fresh.length; i++) Object.assign(root.rows[i], fresh[i])
            root.patchRev = root.patchRev + 1
            return
        }
        root.rows = fresh
        root.rowsKey = sig
        root.patchRev = root.patchRev + 1
        // `ensureExpanded` is idempotent (cached rows are skipped). Rust fires
        // it from `detailSetViewMode("expanded")`; the OTHER entry is an
        // INITIAL OPEN whose restored view-pref is already `expanded`
        // (spec 01 §7.5), which no invokable covers. Read off `d`, not the
        // `doc` binding, which may not have re-evaluated yet.
        if (d.loading !== true && d.found === true && d.viewMode === "expanded"
            && root.expandedFor !== (d.id || "")) {
            root.expandedFor = d.id || ""
            QbzMyQbz.ensureExpanded()
        }
    }
    property string expandedFor: ""
    /// The single republish entry point: parse once, publish `doc`, then patch
    /// or replace `rows` off THAT same object. Never read `root.doc` here — the
    /// order between a binding on `doc` and this handler is not defined, which
    /// is why the parse happens locally and is handed to both.
    function refresh() {
        var d = root.parseDoc()
        root.doc = d
        root.syncRows(d)
    }
    Connections {
        target: QbzMyQbz
        function onDetailJsonChanged() { root.refresh() }
    }
    // `doc`'s initial binding has already parsed at this point — reuse it
    // instead of parsing a third time.
    Component.onCompleted: root.syncRows(root.doc)

    // --- ONE loading-dots clock for the whole view (spec 01 §7.5 / §15.5) --
    // Slint animates each row's LoadingDots off `animation-tick`; a windowed
    // list of hundreds of rows must not. Gating rule, copied from
    // `views/PlaylistView.qml:156-179`: freeze when not visible or the window
    // is minimized / hidden, NEVER on lost focus (a tiling desktop keeps
    // windows visible and unfocused).
    property int dotPhase: 0
    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true
    readonly property bool anyResolving: {
        if (root.patchRev < 0) return false      // `patchRev` dependency
        var r = root.rows
        for (var i = 0; i < r.length; i++) {
            if (r[i].qualityResolving === true && (r[i].qualityTier || "") === "") return true
        }
        return false
    }
    Timer {
        interval: 300
        repeat: true
        running: root.anyResolving && root.visible && root.windowShowing
        onTriggered: root.dotPhase = (root.dotPhase + 1) % 3
    }

    // ============================== LOADING ==============================
    QbzSpinner {
        visible: root.loading
        anchors.centerIn: parent
        size: 36
    }

    // ============================= NOT FOUND =============================
    Text {
        visible: !root.loading && !root.found
        anchors.centerIn: parent
        text: root.trs("Not found")
        color: theme.textMuted
        font.pixelSize: theme.fontLink
    }

    // =============================== LOADED ==============================
    Item {
        id: page
        visible: !root.loading && root.found
        anchors.fill: parent

        // --------------------------- fixed header -------------------------
        Column {
            id: headerCol
            x: 32
            y: 16
            width: parent.width - 64
            spacing: 0

            // --- Hero (.slint :663-861) -------------------------------
            Row {
                width: parent.width
                spacing: 32

                // Rust pre-downscales the hero cells (3 cols -> _50, else
                // _150 — `myqbz_detail.rs:375`), so the mosaic only lays out.
                CollectionMosaic {
                    anchors.bottom: parent.bottom
                    size: 186
                    kind: root.kind
                    itemCount: root.itemCount
                    hasCustomCover: root.doc.hasCustomCover === true
                    customCoverPath: root.doc.customCoverPath || ""
                    coverCount: root.doc.coverCount || 0
                    urls: root.doc.heroCellUrls || []
                    paths: root.doc.heroCellPaths || []
                }

                Column {
                    width: Math.max(0, parent.width - 186 - 32)
                    anchors.bottom: parent.bottom
                    spacing: 8

                    Text {
                        text: root.doc.kindLabel || ""
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 1.2
                    }
                    // 18px, NOT 25 — .slint :709-713 calls the old 25px a
                    // homologation bug against the other detail heroes.
                    Text {
                        width: parent.width
                        text: root.doc.name || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        visible: (root.doc.description || "") !== ""
                        width: Math.min(root.width - 280, 720)
                        text: root.doc.description || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLink
                        wrapMode: Text.WordWrap
                    }
                    Text {
                        text: root.doc.meta || ""
                        color: theme.textMuted
                        font.pixelSize: theme.fontLegal
                    }
                    Item { width: 1; height: 4 }

                    // EXACTLY four controls (.slint :735-859).
                    Row {
                        spacing: 8
                        QbzCircleAction {
                            name: "play-fill"
                            primary: true
                            btnEnabled: root.itemCount > 0
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzMyQbz.playAll()
                        }
                        QbzCircleAction {
                            name: "shuffle"
                            btnEnabled: root.itemCount > 0
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzMyQbz.shuffle()
                        }
                        // ASSET GAP: `turntable` is not baked yet (spec 01 §14).
                        QbzCircleAction {
                            name: "turntable"
                            btnEnabled: root.itemCount > 0
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzMyQbz.djMix()
                        }
                        Item {
                            id: overflowAnchor
                            width: 32
                            height: 32
                            anchors.verticalCenter: parent.verticalCenter
                            // Self-sizing (32px, the non-primary arm), so the
                            // anchor Item is exactly its box.
                            QbzCircleAction {
                                name: "ellipsis"
                                // Slint anchors this 220px panel `x: 0,
                                // y: 36` on the 32px disc (:787-788), so it
                                // hugs the trigger's LEFT edge —
                                // `openBelowRight` would throw it 188px left.
                                onClicked: heroMenu.openBelowLeft(overflowAnchor)
                            }
                        }
                    }
                }
            }

            Item { width: 1; height: 32 }

            // --- Empty collection (.slint :866-877) — the hero stays, the
            // toolbar and the body do not (:880) ----------------------------
            Item {
                visible: root.itemCount === 0
                width: parent.width
                height: visible ? 80 : 0
                Text {
                    anchors.centerIn: parent
                    text: root.trs("No items yet. Add albums, tracks, or playlists from their detail pages.")
                    color: theme.textMuted
                    font.pixelSize: 13
                }
            }

            // --- Sticky toolbar (.slint :885-1086) ------------------------
            Item {
                visible: root.itemCount > 0
                width: parent.width
                height: visible ? 30 : 0

                Row {
                    anchors.right: parent.right
                    height: parent.height
                    spacing: 8

                    QbzLineEdit {
                        anchors.verticalCenter: parent.verticalCenter
                        searchMode: true
                        expandable: true
                        sm: true
                        elevated: false
                        openWidth: 196
                        placeholder: root.trs("Search")
                        text: root.doc.search || ""
                        onEdited: function (v) { QbzMyQbz.detailSearch(v) }
                    }

                    QbzSelect {
                        anchors.verticalCenter: parent.verticalCenter
                        sm: true
                        menuWidth: 170
                        options: [root.trs("Position"), root.trs("Name"),
                                  root.trs("Year"), root.trs("Tracks")]
                        currentIndex: (root.doc.sort || "position") === "name" ? 1
                            : (root.doc.sort || "position") === "year" ? 2
                            : (root.doc.sort || "position") === "tracks" ? 3 : 0
                        onSelected: function (i) {
                            QbzMyQbz.detailSetSort(i === 1 ? "name" : i === 2 ? "year"
                                : i === 3 ? "tracks" : "position")
                        }
                    }

                    // Filter — the LABELED arm, so `fillActive` (accent FILL).
                    QbzToolButton {
                        id: filterBtn
                        anchors.verticalCenter: parent.verticalCenter
                        name: "list-filter"
                        label: root.filterCount > 0
                            ? (root.filterCount === 1 ? root.trs("1 filter")
                               : root.trs("{} filters").replace("{}", root.filterCount))
                            : root.trs("Filter")
                        active: root.filterCount > 0
                        fillActive: true
                        onClicked: filterMenu.openBelowLeft(filterBtn)
                    }

                    // Reset — icon-only, so accent BORDER, never a fill.
                    QbzToolButton {
                        visible: root.hasAnyFilter
                        anchors.verticalCenter: parent.verticalCenter
                        name: "rotate-ccw"
                        fillActive: false
                        onClicked: QbzMyQbz.detailResetFilters()
                    }

                    QbzToolButton {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "square-check-big"
                        active: root.selectMode
                        fillActive: false
                        onClicked: QbzMyQbz.detailToggleSelectMode()
                    }

                    QbzSegToggle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 90
                        mode: root.viewMode
                        segments: [{ "id": "list", "icon": "list" },
                                   { "id": "grid", "icon": "layout-grid" },
                                   { "id": "expanded", "icon": "rows-3" }]
                        // The 3-segment ViewToggle metric set, NOT the control's
                        // Local defaults: MixtapeDetailView.slint:1058-1084 is a
                        // 90x30 r6 surface-elevated well holding THREE
                        // `ToggleButton { sm }` edge to edge — 30x30 each, no
                        // gap, 15px glyph, active fill surface-hover (never
                        // accent), hover surface-elevated, tint text-primary /
                        // text-muted (ToggleButton.slint:19-20,24-28,32-33,38).
                        // `segRadius` 6 rather than the reference's 0 because a
                        // QML clip is rectangular — see QbzSegToggle.qml.
                        segWidth: 30
                        segHeight: 30
                        segRadius: 6
                        segSpacing: 0
                        glyphSize: 15
                        activeFill: theme.surfaceHover
                        hoverFill: theme.surfaceElevated
                        activeTint: "textPrimary"
                        idleTint: "muted"
                        onSetMode: function (m) { QbzMyQbz.detailSetViewMode(m) }
                    }
                }
            }

            Item { visible: root.itemCount > 0; width: 1; height: visible ? 10 : 0 }
        }

        // ----------------------------- the body ----------------------------
        Item {
            id: body
            visible: root.itemCount > 0
            anchors.top: headerCol.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.leftMargin: 32
            anchors.rightMargin: 32
            anchors.bottomMargin: 100

            // ================= BODY: LIST / EXPANDED =================
            Column {
                id: listHost
                anchors.fill: parent
                visible: root.viewMode !== "grid"
                spacing: 0

                Rectangle { width: parent.width; height: 1; color: theme.surfaceElevated }

                // Column header (.slint :1114-1135) — the 8th cell is a bare
                // 40px spacer, not a label (:1130).
                Item {
                    width: parent.width
                    height: 34
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 12
                        ColHead { width: 40; text: "#"; horizontalAlignment: Text.AlignHCenter }
                        ColHead {
                            width: Math.max(0, listHost.width - 700)
                            text: root.trs("Item")
                        }
                        ColHead { width: 140; text: root.trs("Type") }
                        ColHead { width: 80; text: root.trs("Source") }
                        ColHead { width: 160; text: root.trs("Quality") }
                        ColHead { width: 72; text: root.trs("Tracks"); horizontalAlignment: Text.AlignRight }
                        ColHead { width: 60; text: root.trs("Year"); horizontalAlignment: Text.AlignRight }
                        Item { width: 40; height: 1 }
                    }
                }

                Rectangle { width: parent.width; height: 1; color: theme.surfaceElevated }

                Item {
                    width: parent.width
                    height: Math.max(0, listHost.height - 36)

                    ListView {
                        id: itemList
                        anchors.fill: parent
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        cacheBuffer: 500
                        reuseItems: true
                        model: root.rows

                        // EXPANDED mode is this SAME recycled list with the
                        // inline block unhidden — a nested ListView inside a
                        // recycled delegate cannot size itself (spec 02 §9.1),
                        // and switching the delegate per mode would reset
                        // contentY.
                        delegate: Column {
                            id: rowCell
                            required property var modelData
                            required property int index
                            width: itemList.width
                            spacing: 0

                            /// THE LIVE ROW, and the reason every read below goes
                            /// through it instead of `modelData`.
                            ///
                            /// `model: root.rows` is a JavaScript array, and
                            /// QQmlDelegateModel SNAPSHOTS such a model — the
                            /// delegate's `modelData` is the engine's own copy of
                            /// the element, not the object in `root.rows`. So
                            /// `syncRows`' in-place `Object.assign` patch (the
                            /// whole point of which is to update a row WITHOUT
                            /// reassigning the array and throwing the user's
                            /// scroll position to the top) mutated objects the
                            /// delegates could never see. Bumping `patchRev` then
                            /// dutifully re-evaluated every binding — against the
                            /// stale copy.
                            ///
                            /// Measured: 34 items resolved Rust-side with correct
                            /// tiers and 34 publishes, 71 QML refreshes, and the
                            /// Quality column stayed on its skeleton forever.
                            /// Expanded mode rendered nothing for the same reason
                            /// (`inlineTracks` is patched in exactly this way).
                            ///
                            /// Indexing the live array closes it: `patchRev` is
                            /// the dependency, `root.rows[index]` is the object
                            /// Rust actually patched. `modelData` stays the
                            /// fallback for the frame where the array and the
                            /// delegate model are momentarily out of step.
                            readonly property var live: root.patchRev >= 0
                                ? (root.rows[index] || modelData) : modelData

                            readonly property bool expanded: root.viewMode === "expanded"
                                && (root.patchRev >= 0 && rowCell.live.canExpand === true)
                            readonly property var inlineTracks: (root.patchRev >= 0)
                                ? (rowCell.live.inlineTracks || []) : []
                            readonly property bool expandLoading: root.patchRev >= 0
                                && rowCell.live.expandLoading === true
                            readonly property bool tracksLoaded: root.patchRev >= 0
                                && rowCell.live.tracksLoaded === true

                            MyQbzDetailRow {
                                width: parent.width
                                item: rowCell.live
                                ordinal: rowCell.index + 1
                                selectMode: root.selectMode
                                dotPhase: root.dotPhase
                                rev: root.patchRev
                            }

                            // Inline tracks (.slint :1151-1218): padding
                            // 52 / 12 / 4 / 8, spacing 0.
                            Item {
                                visible: rowCell.expanded
                                width: parent.width
                                height: visible ? inlineCol.height + 12 : 0
                                Column {
                                    id: inlineCol
                                    x: 52
                                    y: 4
                                    width: Math.max(0, parent.width - 64)
                                    spacing: 0

                                    Item {
                                        visible: rowCell.expandLoading
                                            && rowCell.inlineTracks.length === 0
                                        width: parent.width
                                        height: visible ? 32 : 0
                                        QbzSpinner { anchors.centerIn: parent; size: 16 }
                                    }
                                    Item {
                                        visible: !rowCell.expandLoading && rowCell.tracksLoaded
                                            && rowCell.inlineTracks.length === 0
                                        width: parent.width
                                        height: visible ? 32 : 0
                                        Text {
                                            anchors.centerIn: parent
                                            text: root.trs("No results found")
                                            color: theme.textMuted
                                            font.pixelSize: 12
                                        }
                                    }
                                    Repeater {
                                        model: rowCell.inlineTracks
                                        delegate: TrackRow {
                                            required property var modelData
                                            required property int index
                                            width: inlineCol.width
                                            item: modelData
                                            // `number` is a DELEGATE property,
                                            // never an item field (spec 02 §5.2).
                                            number: index + 1
                                            showArtwork: false
                                            showFavorite: false
                                            showDownload: false
                                            showMenu: true
                                            // `showAlbum` is LEFT AT ITS DEFAULT
                                            // (false) — TrackRow.slint:43 does the
                                            // same and MixtapeDetailView does not
                                            // override it, so there is no album
                                            // LINK on these rows.
                                            draggable: false
                                            menuShowRemove: false
                                            // `primitives/TrackRow.slint:125`
                                            // zebras UNCONDITIONALLY on odd
                                            // `index` (there is no `zebra`
                                            // property to turn it off), so the
                                            // inline block IS striped in the
                                            // reference. Qt gates it and
                                            // defaults false; `number % 2 === 0`
                                            // with `number = index + 1` stripes
                                            // exactly the same rows.
                                            zebra: true
                                            // Go-to must route through the PARENT
                                            // item, not the track's own ids —
                                            // spec 01 §7.5 / OQ#9.
                                            routeGoToExternally: true
                                            onPlayRequested: QbzMyQbz.inlineTrackAction(
                                                rowCell.modelData.sourceItemId || "",
                                                modelData.id || "", "play")
                                            // `next|later|queue` -> the wire's
                                            // `play-next|play-later|queue`.
                                            // Unmapped is a SILENT no-op.
                                            onEnqueueRequested: function (mode) {
                                                QbzMyQbz.inlineTrackAction(
                                                    rowCell.modelData.sourceItemId || "",
                                                    modelData.id || "",
                                                    mode === "next" ? "play-next"
                                                        : mode === "later" ? "play-later" : "queue")
                                            }
                                            onGoToRequested: function (goKind) {
                                                if (goKind === "artist")
                                                    QbzMyQbz.openArtist(
                                                        rowCell.modelData.source || "",
                                                        modelData.artist || "",
                                                        modelData.artistId || "")
                                                else
                                                    QbzMyQbz.inlineTrackAction(
                                                        rowCell.modelData.sourceItemId || "",
                                                        modelData.id || "", "go-to-album")
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                }
            }

            // ======================= BODY: GRID =======================
            // `auto-fill minmax(150, 1fr)`, gap 20, 12px content inset
            // (.slint :1090-1108). The clipped host + `width: parent.width +
            // 20 - 24` reproduces Slint's `floor((W - 24 + gap) / (150 + gap))`
            // column count, which GridView's own `floor(W / cellWidth)` would
            // otherwise undercount by one at every exact boundary.
            Item {
                id: gridHost
                anchors.fill: parent
                visible: root.viewMode === "grid"
                clip: true

                readonly property int columns: Math.max(1,
                    Math.floor((width - 24 + 20) / (150 + 20)))
                // Clamped: the first frame has width 0, and a negative
                // cellWidth is a silent GridView layout trap.
                readonly property real cardW: Math.max(1,
                    (width - 24 - 20 * (columns - 1)) / columns)
                readonly property real cardH: cardW + 58

                GridView {
                    id: itemGrid
                    x: 12
                    y: 0
                    width: Math.max(0, gridHost.width + 20 - 24)
                    height: gridHost.height
                    cellWidth: gridHost.cardW + 20
                    cellHeight: gridHost.cardH + 20
                    cacheBuffer: 500
                    reuseItems: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: root.rows

                    delegate: MyQbzDetailCard {
                        required property var modelData
                        required property int index
                        // Live row, not the delegate model's snapshot — see the
                        // list delegate's `live` note.
                        item: root.patchRev >= 0 ? (root.rows[index] || modelData) : modelData
                        cardW: gridHost.cardW
                        selectMode: root.selectMode
                        rev: root.patchRev
                    }
                }
            }
        }

        // The .slint floats its ListScrollbar at `parent.width - 14 - 4`
        // (:1226-1235), i.e. INSIDE the page's 32px right padding. Mounted
        // here, outside `body`, because the grid host clips.
        QbzScrollBar {
            target: itemList
            visible: root.itemCount > 0 && root.viewMode !== "grid"
                && itemList.contentHeight > itemList.height
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.top: body.top
            anchors.bottom: body.bottom
        }
        QbzScrollBar {
            target: itemGrid
            visible: root.itemCount > 0 && root.viewMode === "grid"
                && itemGrid.contentHeight > itemGrid.height
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.top: body.top
            anchors.bottom: body.bottom
        }
    }

    // ========================= BULK ACTION BAR ==========================
    // A SIBLING of the scroller, floating over the body's 100px bottom
    // gutter. `parent.width - 18 - 22` is asymmetric on purpose (.slint :1253).
    QbzMultiSelectBar {
        visible: root.selectMode && root.selectedCount > 0
        x: 18
        y: parent.height - height - 12
        width: parent.width - 40
        selectedCount: root.selectedCount
        actions: root.bulkActions()
        onAction: function (id) { QbzMyQbz.bulkAction(id) }
    }

    // Order is the .slint's (:1258-1266) and it puts LATER before NEXT — the
    // opposite of the row menu. Do not "fix" it.
    function bulkActions() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        return [
            { "id": "add-to-queue", "label": t("Add to queue", r), "icon": "list-end",
              "danger": false, "needsSelection": true },
            { "id": "play-later", "label": t("Play later", r), "icon": "list-plus",
              "danger": false, "needsSelection": true },
            { "id": "play-next", "label": t("Play next", r), "icon": "list-start",
              "danger": false, "needsSelection": true },
            { "id": "add-to-playlist", "label": t("Add to playlist", r), "icon": "list-music",
              "danger": false, "needsSelection": true },
            { "id": "add-to-mixtape", "label": t("Add to Mixtape/Collection", r),
              "icon": "cassette-tape", "danger": false, "needsSelection": true },
            { "id": "remove-selected", "label": t("Remove", r), "icon": "trash-2",
              "danger": true, "needsSelection": true },
            { "id": "clear", "label": t("Clear", r), "icon": "x",
              "danger": false, "needsSelection": true },
        ]
    }

    // ========================== HERO ⋯ MENU =============================
    CardMenu {
        id: heroMenu
        menuWidth: 220
        entries: root.heroMenuModel()
        onPicked: function (a) { root.heroMenuAction(a) }
    }

    function heroMenuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        // ASSET GAP: `image` is not baked yet (spec 01 §14).
        var m = [
            { "label": t("Rename", r), "icon": "pen-line", "action": "rename" },
            { "label": t("Edit description", r), "icon": "pen-line", "action": "description" },
            { "label": t("Upload cover", r), "icon": "image", "action": "upload-cover" },
        ]
        if (root.doc.hasCustomCover === true)
            m.push({ "label": t("Clear custom cover", r), "icon": "x", "action": "clear-cover" })
        m.push({ "sep": true })
        // The label is the OTHER mode (.slint :831-837).
        m.push({ "label": (root.doc.playMode || "in_order") === "in_order"
                    ? t("Album shuffle", r) : t("In order", r),
                 "icon": "shuffle", "action": "play-mode" })
        if (root.kind !== "artist_collection")
            m.push({ "label": root.kind === "mixtape"
                        ? t("Convert to Collection", r) : t("Convert to Mixtape", r),
                     "icon": "cassette-tape", "action": "convert" })
        m.push({ "sep": true })
        m.push({ "label": t("Delete", r), "icon": "trash-2", "action": "delete", "danger": true })
        return m
    }

    function heroMenuAction(a) {
        if (a === "rename") QbzMyQbz.openRename()
        else if (a === "description") QbzMyQbz.openDescription()
        else if (a === "upload-cover") QbzMyQbz.uploadCover()
        else if (a === "clear-cover") QbzMyQbz.removeCover()
        else if (a === "play-mode") QbzMyQbz.togglePlayMode()
        else if (a === "convert") QbzMyQbz.convertKind()
        else if (a === "delete") QbzMyQbz.openDelete()
    }

    // ========================= FILTER POPUP =============================
    // `QbzContextMenu` HARDCODES padding 5 / `surfaceMain` / r8 /
    // `borderMuted` and its content Column's spacing is unreachable through
    // the `menuContent` alias, so the panel, the padding and the row spacing
    // are all written here (spec 01 §3.3). Its default
    // `CloseOnPressOutside` is what the .slint asks for (:952-956): these are
    // radio / checkbox rows that must survive several toggles.
    QbzContextMenu {
        id: filterMenu
        menuWidth: 200
        padding: 4
        background: Rectangle {
            color: theme.surfaceMain
            radius: 8
            border.width: 1
            border.color: theme.surfaceElevated
        }
        Column {
            width: parent ? parent.width : 0
            spacing: 1

            SectionHead { text: root.trs("Type") }
            MenuOption {
                label: root.trs("All types")
                selected: (root.doc.typeFilter || "all") === "all"
                onChosen: QbzMyQbz.detailSetTypeFilter("all")
            }
            MenuOption {
                label: root.trs("Album")
                selected: root.doc.typeFilter === "album"
                onChosen: QbzMyQbz.detailSetTypeFilter("album")
            }
            MenuOption {
                label: root.trs("Track")
                selected: root.doc.typeFilter === "track"
                onChosen: QbzMyQbz.detailSetTypeFilter("track")
            }
            MenuOption {
                label: root.trs("Playlist")
                selected: root.doc.typeFilter === "playlist"
                onChosen: QbzMyQbz.detailSetTypeFilter("playlist")
            }
            Rectangle {
                width: parent.width
                height: 1
                color: theme.surfaceElevated
            }
            SectionHead { text: root.trs("Source") }
            MenuOption {
                label: root.trs("Qobuz")
                selected: root.doc.srcQobuz === true
                onChosen: QbzMyQbz.detailToggleSourceFilter("qobuz")
            }
            MenuOption {
                label: root.trs("Plex")
                selected: root.doc.srcPlex === true
                onChosen: QbzMyQbz.detailToggleSourceFilter("plex")
            }
            MenuOption {
                label: root.trs("Local")
                selected: root.doc.srcLocal === true
                onChosen: QbzMyQbz.detailToggleSourceFilter("local")
            }
        }
    }

}
