// Left navigation sidebar — QML port of crates/qbz-ui/ui/shell/Sidebar.slint.
//
// Three states (ShellState.sidebar-state): 0 = open 240px (icon + label),
// 1 = mini (icons only), 2 = closed 0px. Width animates 160ms ease-in-out;
// the header's panel-left button cycles (QbzShell.cycleSidebar).
//
// MINI RAIL: 64px, 1:1 with Slint (state.slint:4075), holding a 34px row
// (Sidebar.slint:575/652) on symmetric 15px padding. It was 50px for a while —
// an owner deviation that turned out to put the rail's centre 5px left of the
// header control sitting directly above it; see `miniPadLeft` below and
// QbzTheme.sidebarMiniWidth for the measurement. Everything else
// in the mini state is 1:1 with Slint: rows centre their lone glyph, nested
// playlist rows collapse to zero, hovering a row names it in a bubble to the
// right (SidebarTooltip.slint) and clicking a FOLDER opens its playlists in a
// flyout to the right (SidebarFolderPopup.slint) — in mini a folder cannot
// expand in place, so a toggle there would be a control that does nothing.
//
// FILE LENGTH (>500): what remains after the mini folder flyout moved to
// shell/SidebarFolderFlyout.qml and the row context menu to
// shell/SidebarRowMenu.qml is ONE surface with one state machine — the three
// sidebar widths. Every block below is a WIDTH-CONDITIONAL variant of the same
// tree (the nav rows, the PLAYLISTS toolbar, the row delegate and the
// scrollbar each read `root.mini`), so splitting them into files would hand
// each half a `mini` property to keep in sync and put the row delegate's
// drop-target arithmetic — which needs `root`'s own window coordinates — a
// file away from the root it measures. The three things that DID come out are
// the three that are self-contained: both overlays and the hover bubble (the
// shell's shared QbzTooltip). All three are shared single instances declared
// at this root, never one per row: the tree can hold hundreds of rows.
//
// Top-level section nav rows (Discover / Library / Local Library / My QBZ)
// replicate SidebarNavRow: 34px rows, radius 6, 16px icons, 13px/w500
// labels, surface-hover on hover, Discover + Library HIDDEN while offline
// (ADR-010 mount-site gating). The whole block — rows AND the hairline under
// them — is mounted only while `QbzShell.navInSidebar` is ON; with it OFF the
// sections move to the header (HeaderBar.qml) and this sidebar starts at the
// PLAYLISTS toolbar, exactly like Sidebar.slint:724/817.
//
// The rows open the same dropdown FLYOUT their Slint counterparts do
// (SidebarNavRow: hover or click, panel to the RIGHT of the row, headed by
// the section name) — the panel, its behaviour and the entry catalog live in
// the shared NavFlyout.qml, which HeaderBar.qml mounts too, so the sections
// behave identically wherever the nav is hosted. The rows keep the
// SidebarDirectRow active treatment (surface-hover bg + primary text/icon)
// for the current section and while their own menu is open.
// All four sections route into real views, My QBZ included (its Collections /
// Mixtapes entries reach the "collections" / "mixtapes" routes). The disabled
// treatment the NavRow still carries — dimmed to 0.45, pointer-inert, no
// flyout — is the mechanism for a section with no surface behind it, and
// nothing uses it today (a row that clicks into nothing is a defect, not a
// stub). Its leading glyph goes through shell/NavSectionGlyph.qml, because My
// QBZ can carry a user-supplied icon (SidebarNavRow `raw-icon`).
// The playlist/folder tree below the nav IS live (sidebar_qt.rs: load, sort,
// search, expand/collapse, drag-drop target), and so is everything the "..."
// menu offers: New folder opens QbzFolderEdit's create panel (the small
// name-only create modal, controls/FolderModals.qml), Manage playlists routes
// to the Playlist Manager, Import opens the importer and Refresh rebuilds the
// tree. Import is the only row that dims — it is the only Qobuz-only one, so it
// follows the reference's `enabled: !offline`; Refresh stays lit because its
// bridge call is the offline-safe rebuild (see the Repeater's comment). Nothing
// in this file renders a control that no-ops.
//
// Right-pressing a row opens shell/SidebarRowMenu.qml (edit / hide / mixtape /
// move-to-folder). It is suppressed on the mini rail: Slint anchors that menu
// at `row.width - 210px`, which on a 34px rail row is deeply negative and Qt's
// viewport clamp would drop it at x=8 — not the reference's placement, just
// the clamp.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    property bool mini: QbzShell.sidebarState === 1
    // The shell's ONE hover-tooltip overlay (controls/QbzTooltip.qml), fed in
    // by AppShell. Null when this component is mounted in isolation, in which
    // case the rail simply shows no bubble.
    property Item tooltip: null

    // ---- Collapsed-rail optical centring ---------------------------------
    // MEASURED, not nudged (release binary, 1600x1000, xcb, pixels sampled off
    // the framebuffer). The finding that mattered: the rail's content was
    // ALREADY symmetric on the sidebar's own geometry — hover pill centre 25.0
    // against a rail centre of 25.0 — and the owner still read it as off. The
    // eye was not comparing the glyphs to the rail. It was comparing them to
    // the header's leading control, the panel-left sidebar toggle, which sits
    // at x = spacingMd(16) and is 28px wide (HeaderBar.qml:225 +
    // QbzNavButton.qml:102) -> centre 30, directly above them. Five pixels
    // apart in one vertical line is visible; symmetric padding does not help.
    //
    // Nor is there a visible rail edge to centre against: the sidebar is
    // surface-card and the content frame's 8px left gutter is surface-card too
    // (AppShell.qml:149 paints the panel; the frame around it is the shell
    // base), so it reads as one uninterrupted band, sampled (26,26,26) from
    // x=1 to x=57 and (15,15,15) from 58 — band centre 29.
    //
    // The rail is back at Slint's 64px, which dissolves the conflict instead
    // of trading one misalignment for another: row centre 32 vs header button
    // 30. See QbzTheme.sidebarMiniWidth for why 50 was the actual defect.
    // Padding is symmetric and DERIVED, so the pair can never drift apart.
    readonly property int miniPadLeft: (theme.sidebarMiniWidth - theme.sidebarMiniRow) / 2
    readonly property int miniPadRight: root.miniPadLeft
    // Which section owns the highlight. DERIVED from the live view (in
    // NavFlyout, shared with the header), not set on click: the same rows can
    // live here or in the header, and history navigation (back/forward) must
    // keep the highlight honest. Slint additionally highlights the row whose
    // menu is open — the rows OR `navFlyout.openId` in for that.
    readonly property string activeNav: navFlyout.activeSection
    // Playlist tree state (phase 7).
    property bool searchOpen: false
    property bool playlistsCollapsed: false
    property string activePlaylistId: ""
    // ---- Large-NPB dock space reservation -------------------------------
    // The cover+spectrum dock is an AppShell-ROOT overlay pinned flush to the
    // WINDOW bottom-left, so it extends the bar's height BELOW this sidebar
    // (which stops at npb.top). Only the IN-SIDEBAR portion has to be
    // reserved, and it shrinks when the band is hidden — that is why the
    // height comes from the bridge and not a literal.
    //
    // Without this the playlist list keeps running to the sidebar's bottom
    // edge and the rows sit UNDER the album art (Sidebar.slint:1145 reserves
    // the same way).
    readonly property bool largeDockActive: QbzShell.npbMode === 3 && QbzShell.sidebarState === 0
    readonly property real dockReserve: largeDockActive
        ? Math.max(0, QbzShell.largeDockHeight - theme.npbLargeHeight)
        : 0

    // Flattened entries from the bridge + the url-keyed cover map.
    readonly property var entries: parseEntries(QbzShell.sidebarJson)
    // A republish rebuilds the Repeater's delegates, so the row a bubble is
    // anchored to may be gone or moved and its MouseArea will never report the
    // leave. The overlay outlives the delegate by design (it stores numbers,
    // never the Item), so nothing dangles — but it would linger, so drop it.
    onEntriesChanged: if (root.tooltip) root.tooltip.hideAll()
    property var coverMap: ({})
    function parseEntries(json) {
        var e = JSON.parse(json)
        // Collect every cover url and dispatch the artwork window.
        var urls = []
        for (var i = 0; i < e.length; i++) {
            for (var j = 0; j < e[i].covers.length; j++) urls.push(e[i].covers[j])
        }
        if (urls.length > 0) QbzShell.sidebarArtworkWindow(JSON.stringify(urls))
        return e
    }

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }

    QbzTheme { id: theme }

    // The section-nav dropdown (catalog + panel + open/close behaviour) —
    // the SAME component HeaderBar.qml mounts. Zero-size and outside the
    // Column so its origin stays this root's origin (the row coordinates are
    // mapped into it). The panel renders in the window overlay, so neither
    // this root's `clip` nor its 240px width can cut it off.
    NavFlyout { id: navFlyout }

    // ---- Collapsed (mini) affordances -----------------------------------
    // In the mini rail a playlist row is an icon and nothing else, so the two
    // things the open sidebar gives for free have to be restored:
    //   - the NAME, as a hover bubble to the row's right (SidebarTooltip.slint);
    //   - a folder's CONTENTS, as a flyout to the row's right, because the
    //     nested rows are hidden in mini (SidebarFolderPopup.slint).
    // Both are ONE shared instance driven by the hovered/clicked row (exactly
    // like Slint's SidebarTooltipState / SidebarFolderPopupState), never one
    // per delegate: the tree can hold hundreds of rows.
    // Neither can live inside this root's bounds, because it clips. The FLYOUT
    // is a QtQuick.Controls Popup, so its content is reparented to the WINDOW
    // overlay — the same reason NavFlyout and HeaderBar's playlists flyout are
    // Popups. The BUBBLE is not a popup at all: it is the shell-level
    // QbzTooltip overlay (`root.tooltip`, fed in by AppShell), for the reasons
    // spelled out where the old ToolTip used to be declared, below.

    // Race-safe show/hide (Slint SidebarRow.show-tooltip / hide-tooltip): the
    // leaving row only clears the bubble if it still owns it, so sliding the
    // pointer straight from one row to the next never blanks it. Both halves
    // now live in the shared overlay, keyed by the entry id.
    function showMiniTip(row, entry) {
        if (!root.mini || !root.tooltip)
            return
        root.tooltip.showRight(row, entry.id, entry.kind === "folder"
            ? entry.name + " (" + entry.count + ")"
            : entry.name)
    }
    function hideMiniTip(id) {
        if (root.tooltip)
            root.tooltip.hide(id)
    }

    // Mini folder click: the flyout, not a toggle (the nested rows are hidden,
    // so toggling is a control that renders and does nothing — Slint opens the
    // popup instead, Sidebar.slint:355-377).
    //
    // It no longer force-expands the folder on the way in. That used to be
    // necessary because the flyout filtered the published `entries`, which
    // carry a folder's children ONLY while it is expanded — a persistent side
    // effect (the folder stayed open once the sidebar was re-opened) the
    // reference does not have. The flyout now asks Rust for a dedicated
    // cache-built document instead (SidebarFolderFlyout.qml / §4.7).
    function openFolderFlyout(row, entry) {
        // The flyout takes the same +6px slot the bubble occupies, and it is a
        // Popup (window overlay) so it would paint straight over it. Slint drops
        // the tooltip explicitly here too (Sidebar.slint:353).
        if (root.tooltip) root.tooltip.hideAll()
        folderFlyout.openFor(entry, row)
    }

    // Cycling the sidebar while any of the three is open would leave it
    // floating over a rail that no longer exists (the bubble self-hides
    // through `root.mini`; the two Popups need telling).
    Connections {
        target: QbzShell
        function onSidebarStateChanged() {
            folderFlyout.close()
            // The row menu is row-anchored and the rows are about to change
            // width — or, going to state 2, stop existing.
            rowMenu.close()
            // The bubble's anchor is a rail row that is about to move or stop
            // existing (SidebarTooltip.slint:14 gates on sidebar-mini for the
            // same reason).
            if (root.tooltip) root.tooltip.hideAll()
        }
    }

    // The hover bubble that names a collapsed row is NOT declared here any
    // more: it is the shell's shared overlay (controls/QbzTooltip.qml, mounted
    // once by AppShell), which this file drives through showMiniTip /
    // hideMiniTip above. Two reasons, in order of severity:
    //
    //  1. THE OLD ONE WAS BROKEN. It was a QtQuick.Controls `ToolTip` with a
    //     custom contentItem, and a ToolTip does not propagate a custom
    //     content's implicit size: measured on Qt 6.11.1, an identical Text
    //     inside a `Popup` gives implicitContentWidth 157.8 and inside a
    //     `ToolTip` gives 1. The Basic style then sizes the popup to
    //     `implicitContentWidth + padding`, so the bubble collapsed to ~13x25
    //     while the Text — which a Popup never clips — kept painting its full
    //     178px: the label floated over the content pane with no background and
    //     a small detached box sat beside it (owner report, reproduced and
    //     pixel-sampled before the change).
    //  2. Slint does not put a popup per call site either. It has ONE overlay
    //     fed by a global channel (SidebarTooltipState, state.slint:1893;
    //     TooltipState, state.slint:4791), precisely so a clipped surface — and
    //     this sidebar clips — can still show a bubble outside its own bounds.

    // Folder flyout (mini rail) and the row context menu — ONE shared instance
    // each, declared at this root, never one per delegate: the tree can hold
    // hundreds of rows. Both are Popups, so their content is reparented to the
    // WINDOW overlay and this root's `clip: true` cannot cut them off.
    SidebarFolderFlyout {
        id: folderFlyout
        onPlaylistActivated: function (id) { root.activePlaylistId = id }
    }

    // The canonical folder list, parsed once. `foldersJson` is the Playlist
    // Manager's property because the manager owns the rich folder record, but
    // it is kept fresh independently of the manager's document cache
    // (refresh_folders()), so the sidebar can read it before — and without —
    // the manager view ever being opened.
    readonly property var pmFolders: {
        try { return JSON.parse(QbzPlaylistManager.foldersJson) } catch (e) { return [] }
    }
    // Hidden folders are dropped EVERYWHERE in the sidebar tree, so the row
    // menu must not offer them, must not count them toward its search
    // threshold and must not let them suppress its empty state. Derived once,
    // here, and read three times over there.
    readonly property var visibleFolders: root.pmFolders.filter(function (f) {
        return f.isHidden !== true
    })

    SidebarRowMenu {
        id: rowMenu
        visibleFolders: root.visibleFolders
    }

    width: QbzShell.sidebarState === 2 ? 0
         : QbzShell.sidebarState === 1 ? theme.sidebarMiniWidth
         : theme.sidebarOpenWidth
    // surface-card @ 0.5 while the ambient background is active (phase 14).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
       
    // Square edges; clip keeps content from spilling while the width
    // animates (same as the Slint root).
    clip: true

    Behavior on width {
        NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
    }

    // One section-nav row (SidebarNavRow / SidebarDirectRow metrics). Hovering
    // or clicking it opens that section's flyout to the RIGHT of the row.
    component NavRow: Rectangle {
        id: navRow
        // A section from the shared catalog (NavFlyout.sections).
        property var section: null
        readonly property bool isActive: section
            && (root.activeNav === section.id || navFlyout.openId === section.id)
        // A section with no surface behind it renders as unavailable rather
        // than as a live row that clicks into nothing. Every section is live
        // today; the treatment stays for the next stub.
        readonly property bool isEnabled: section && section.enabled
        readonly property bool lit: isActive || (navArea.containsMouse && isEnabled)

        width: parent ? parent.width : 0
        // Mini: a SQUARE row that fills the 34px padded track (theme token,
        // rail 50 - 2*8). Open keeps Slint's 34px (Sidebar.slint:575).
        height: root.mini ? theme.sidebarMiniRow : 34
        radius: 6
        opacity: isEnabled ? 1.0 : 0.45
        color: lit ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: root.mini ? 0 : 8
            anchors.rightMargin: root.mini ? 0 : 8
            spacing: 10

            Item {
                width: root.mini ? parent.width : 16
                height: parent.height
                // Baked glyph, or the section's own raw image when it carries
                // one (My QBZ branding) — Sidebar.slint:601-612.
                NavSectionGlyph {
                    section: navRow.section
                    size: 16
                    anchors.centerIn: parent
                    tintName: navRow.lit ? "textPrimary" : "secondary"
                }
            }
            Text {
                visible: !root.mini
                height: parent.height
                width: parent.width - (root.mini ? 0 : 26)
                text: navRow.section ? navRow.section.label : ""
                color: navRow.lit ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: navArea
            anchors.fill: parent
            enabled: navRow.isEnabled
            hoverEnabled: navRow.isEnabled
            cursorShape: Qt.PointingHandCursor
            onClicked: navFlyout.openBeside(navRow, navRow.section)
            onContainsMouseChanged: {
                if (containsMouse) {
                    // Hover-to-open; hovering another row overwrites the open
                    // menu (instant switch, single-open by construction).
                    navFlyout.triggerHovered = true
                    navFlyout.openBeside(navRow, navRow.section)
                } else if (navRow.section && navFlyout.openId === navRow.section.id) {
                    // Only the row owning the open menu clears the flag.
                    navFlyout.triggerHovered = false
                }
            }
        }
    }

    Column {
        anchors.fill: parent
        // Mini left/right are ASYMMETRIC on purpose — see `miniPadLeft` at the
        // top of this file for the measurement. Top stays 8 (nothing above the
        // rail competes with it vertically).
        anchors.leftMargin: root.mini ? root.miniPadLeft : theme.spacingMd
        anchors.rightMargin: root.mini ? root.miniPadRight : theme.spacingMd
        anchors.topMargin: root.mini ? 8 : theme.spacingMd
        // Per-side padding so Large can zero ONLY the bottom (Sidebar.slint:719):
        // the reserved dock band then reaches the true window bottom-left and the
        // cover sits flush (the L corner). Left/right/top are unchanged.
        anchors.bottomMargin: root.largeDockActive ? 0 : (root.mini ? 8 : theme.spacingMd)
        spacing: theme.spacingMd

        // ---- Section nav -------------------------------------------
        Column {
            id: navColumn
            // Section-nav placement: OFF moves the whole block to the header.
            // A Column skips invisible children entirely (no phantom spacing),
            // so this is the `if` mount-site gate Slint uses.
            visible: QbzShell.navInSidebar
            width: parent.width
            spacing: 2

            // The same four sections the header hosts, from the same catalog.
            // Qobuz-only ones are HIDDEN entirely while offline (ADR-010
            // mount-site gating, not rendered-disabled). My QBZ is `qobuz:
            // false` in the catalog, i.e. it stays mounted while offline —
            // Slint does not offline-gate its row either
            // (Sidebar.slint:779-792, unlike the Purchases row right after it).
            Repeater {
                model: navFlyout.sections
                delegate: NavRow {
                    required property var modelData
                    section: modelData
                    visible: !(modelData.qobuz && QbzSession.offline)
                }
            }
        }

        // Hairline under the section nav — mounted with the nav block, not
        // on its own (Sidebar.slint:817 gates it on the same flag).
        Rectangle {
            visible: QbzShell.navInSidebar
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ---- Playlists header toolbar (hidden in the mini state) -----
        Row {
            visible: !root.mini
            width: parent.width
            height: 22
            spacing: 4

            // Title (hidden while the search input is open).
            Text {
                visible: !root.searchOpen
                width: parent.width - 4 * 26
                height: parent.height
                text: QbzSession.tr("PLAYLISTS", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 10
                font.letterSpacing: 1
                verticalAlignment: Text.AlignVCenter
            }
            // Inline search input (filters entries, recursive).
            Rectangle {
                visible: root.searchOpen
                width: parent.width - 4 * 26
                height: 22
                radius: 4
                color: theme.surfaceElevated
                border.width: 1
                border.color: theme.borderSubtle
                TextInput {
                    id: searchEdit
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    color: theme.textPrimary
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    clip: true
                    onTextEdited: QbzShell.sidebarSearch(text)
                    Text {
                        visible: searchEdit.text === ""
                        anchors.fill: parent
                        text: QbzSession.tr("Search playlists", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 12
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            // Search toggle.
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: (searchBtnArea.containsMouse || root.searchOpen) ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "search"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: (searchBtnArea.containsMouse || root.searchOpen) ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: searchBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.searchOpen = !root.searchOpen
                        if (!root.searchOpen) {
                            searchEdit.text = ""
                            QbzShell.sidebarSearch("")
                        } else {
                            searchEdit.forceActiveFocus()
                        }
                    }
                }
            }
            // New playlist (+) — single bridge call (create_playlist), then
            // reload the tree. Slint opens a naming modal; the POC creates
            // with the default name (POC-NOTE).
            //
            // NOT offline-gated, and that is deliberate: `crate::create_playlist`
            // branches on connectivity and creates a LOCAL playlist while
            // offline (the reference's D8 — its create modal locks the
            // offline-only toggle ON there). Dimming this row would take the
            // ONE way an account-less user makes a playlist away from exactly
            // the state they live in. Before that branch existed the button was
            // lit and its whole offline effect was a log line.
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: plusArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "plus"
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: plusArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: plusArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzShell.createPlaylist()
                }
            }
            // Sort / more (...) menu.
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: sortBtnArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "ellipsis"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: sortBtnArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: sortBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sortMenu.open()
                }
                Popup {
                    id: sortMenu
                    // Slint puts this panel's left edge at `toolbar.width -
                    // 40px`. Resolved: the open sidebar's content column is
                    // 240 - 2*16 = 208 wide and the "..." button occupies
                    // 160…182 of it, so 208 - 40 = 168 is the button's own
                    // x + 8. This Popup is parented to the BUTTON, not to the
                    // row, so the 1:1 value is x: 8 — the old `parent.width +
                    // 18` (= 40) put it 32px too far right.
                    x: 8
                    y: 26
                    width: 200
                    padding: 6
                    closePolicy: Popup.CloseOnPressOutside
                    background: Rectangle {
                        // Matches this root's own ambient treatment
                        // (Sidebar.slint:920-922).
                        color: root.ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
                        radius: 8
                        border.width: 1
                        border.color: theme.borderSubtle
                    }
                    contentItem: Column {
                        spacing: 1
                        Row {
                            leftPadding: 8
                            topPadding: 4
                            bottomPadding: 2
                            spacing: 6
                            QbzIcon { name: "arrow-up-down"; width: 13; height: 13; tintName: "muted" }
                            Text {
                                text: QbzSession.tr("Sort by", QbzSession.trRev)
                                color: theme.textMuted
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                        Repeater {
                            model: [
                                { "opt": "name", "label": QbzSession.tr("Name (A-Z)", QbzSession.trRev) },
                                { "opt": "recent", "label": QbzSession.tr("Recent", QbzSession.trRev) },
                                { "opt": "tracks", "label": QbzSession.tr("# of tracks", QbzSession.trRev) },
                                { "opt": "playcount", "label": QbzSession.tr("Play Count", QbzSession.trRev) },
                                { "opt": "custom", "label": QbzSession.tr("Custom", QbzSession.trRev) },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 30
                                radius: 5
                                color: sortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    anchors.rightMargin: 8
                                    spacing: 6
                                    Text {
                                        width: parent.width - 20
                                        height: parent.height
                                        text: modelData.label
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        font.weight: QbzShell.sidebarSortBy === modelData.opt ? theme.weightSemibold : theme.weightRegular
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    QbzIcon {
                                        visible: QbzShell.sidebarSortBy === modelData.opt
                                        name: QbzShell.sidebarSortAsc ? "chevron-up" : "chevron-down"
                                        width: 13
                                        height: 13
                                        anchors.verticalCenter: parent.verticalCenter
                                        tintName: "accent"
                                    }
                                }
                                MouseArea {
                                    id: sortOptArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        QbzShell.sidebarSetSort(modelData.opt)
                                        sortMenu.close()
                                    }
                                }
                            }
                        }
                        Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
                        // New folder / Manage playlists / Import / Refresh —
                        // all four LIVE (the Refresh row was missing from this
                        // port entirely and is restored here).
                        //
                        // `on` is the enabled flag and it drives everything at
                        // once: the hover fill, the pointer cursor, the glyph
                        // tint, the 0.4 dim AND whether the MouseArea exists.
                        // There is no way to light one row and leave another
                        // inert without it.
                        //
                        // Import is the only row that can be off, and only
                        // while offline — importing a playlist is Qobuz-only
                        // work.
                        //
                        // REFRESH STAYS LIT OFFLINE (reversed 2026-07-31 on the
                        // owner's smoke). The earlier decision dimmed it
                        // because QbzShell.reloadSidebar() called
                        // crate::reload_sidebar(), which early-returns offline
                        // — a lit row would have been a no-op, and a control
                        // that renders and no-ops is a defect. The bridge now
                        // calls crate::reload_sidebar_including_local()
                        // instead, which offline re-reads folders, folder
                        // membership, the hidden set and the LOCAL playlists
                        // from library.db and republishes (preserving the
                        // cached Qobuz set). That is a real refresh, and it is
                        // precisely the tree an account-less user owns — the
                        // owner hit an empty offline sidebar with no way to
                        // rebuild it.
                        //
                        // Every arm closes the menu FIRST. Slint's PopupWindow
                        // dismisses on any click, including inside itself,
                        // which is why its own items never call close(); Qt's
                        // CloseOnPressOutside does the opposite. This is the
                        // single most common silent divergence in this seam.
                        Repeater {
                            model: [
                                { "icon": "folder-plus", "label": QbzSession.tr("New folder", QbzSession.trRev),
                                  "act": "newFolder", "on": true },
                                { "icon": "library-big", "label": QbzSession.tr("Manage playlists", QbzSession.trRev),
                                  "act": "manage", "on": true },
                                // LIVE. The importer landed with this seam
                                // (src/playlist_import_bridge.rs +
                                // controls/PlaylistImportModal.qml), so contract
                                // §10 Q2 resolves to "land the importer" and
                                // there is no `typeof QbzPlaylistImport` guard
                                // here: the row is a real row, dimmed only
                                // while offline — the only row that dims.
                                { "icon": "import", "label": QbzSession.tr("Import", QbzSession.trRev),
                                  "act": "import", "on": !QbzSession.offline },
                                { "icon": "refresh-cw", "label": QbzSession.tr("Refresh", QbzSession.trRev),
                                  "act": "refresh", "on": true },
                            ]
                            delegate: Rectangle {
                                id: actRow
                                required property var modelData
                                readonly property bool rowEnabled: modelData.on === true
                                width: parent ? parent.width : 0
                                height: 30
                                radius: 5
                                color: (actRow.rowEnabled && actArea.containsMouse)
                                    ? theme.surfaceHover : "transparent"
                                opacity: actRow.rowEnabled ? 1.0 : 0.4
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    anchors.rightMargin: 8
                                    spacing: 8
                                    QbzIcon {
                                        name: actRow.modelData.icon
                                        width: 14
                                        height: 14
                                        anchors.verticalCenter: parent.verticalCenter
                                        tintName: actRow.rowEnabled ? "secondary" : "muted"
                                    }
                                    Text {
                                        width: parent.width - 22
                                        height: parent.height
                                        text: actRow.modelData.label
                                        color: actRow.rowEnabled ? theme.textSecondary : theme.textMuted
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }
                                }
                                MouseArea {
                                    id: actArea
                                    anchors.fill: parent
                                    enabled: actRow.rowEnabled
                                    hoverEnabled: actRow.rowEnabled
                                    cursorShape: actRow.rowEnabled
                                        ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    onClicked: {
                                        sortMenu.close()
                                        var a = actRow.modelData.act
                                        if (a === "newFolder") QbzFolderEdit.openCreate()
                                        else if (a === "manage") QbzPlaylistManager.navigate()
                                        else if (a === "import") QbzPlaylistImport.open()
                                        else if (a === "refresh") QbzShell.reloadSidebar()
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Collapse chevron (down = expanded, right = collapsed).
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: collapseArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: root.playlistsCollapsed ? "chevron-right" : "chevron-down"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: collapseArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: collapseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.playlistsCollapsed = !root.playlistsCollapsed
                }
            }
        }

        // ---- Playlist tree (folders + playlists, sidebar_qt.rs) --------
        Item {
            visible: !root.playlistsCollapsed
            width: parent.width
            // This is the element that FILLS (Sidebar.slint gives the playlist
            // Rectangle `vertical-stretch: 1`): everything above it — the nav
            // block, the hairline, the 22px toolbar — takes its natural height
            // and the list eats the rest, down to the Column's bottom margin.
            // `root.dockReserve` keeps the rows clear of the Large-NPB cover
            // (0 in every other mode). Clamped so a short window shrinks the
            // list to nothing instead of giving it a negative height.
            //
            // There used to be a trailing `Item { height: 0 }` "filler" after
            // this one and a matching `- 16` here to pay for the Column spacing
            // it introduced. Net effect: the list stopped 16px short and the
            // sidebar read as too short against Slint. Both are gone — the
            // list is the last child, so its bottom IS the Column's bottom.
            height: Math.max(0, parent.height - y - root.dockReserve)

            Flickable {
                id: plFlick
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: plColumn.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: plColumn
                    width: parent.width
                    spacing: 2

                    Repeater {
                        model: root.entries
                        delegate: Rectangle {
                            id: plRow
                            required property var modelData
                            property bool isFolder: modelData.kind === "folder"
                            // Slint SidebarRow.use-collage: opt-OUT setting AND
                            // at least one cover; otherwise the list-music glyph.
                            property bool useCollage: !isFolder
                                && QbzShell.sidebarPlaylistCollage
                                && modelData.covers.length > 0
                            property bool isActive: !isFolder && modelData.id === root.activePlaylistId
                            // Track-drag drop target (Sidebar.slint SidebarRow):
                            // the row self-detects the drop from the shared
                            // window-coord pointer and claims/releases it.
                            property bool dropHot: false
                            function recomputeDrop() {
                                if (isFolder || !QbzShell.dragActive) {
                                    if (dropHot) { QbzShell.dragSetOver(""); dropHot = false }
                                    return
                                }
                                const tl = mapToItem(null, 0, 0)
                                const hot = QbzShell.dragX >= tl.x && QbzShell.dragX <= tl.x + width
                                    && QbzShell.dragY >= tl.y && QbzShell.dragY <= tl.y + height
                                if (hot && !dropHot) QbzShell.dragSetOver(modelData.id)
                                else if (!hot && dropHot) QbzShell.dragSetOver("")
                                dropHot = hot
                            }
                            Connections {
                                target: QbzShell
                                function onDragXChanged() { recomputeDrop() }
                                function onDragYChanged() { recomputeDrop() }
                                function onDragActiveChanged() { recomputeDrop() }
                            }
                            width: plColumn.width
                            // Mini: nested rows collapse to nothing (their
                            // folder opens the flyout instead, Sidebar.slint:
                            // 213-215), top-level rows become the SAME square
                            // the nav rows are — a 34px row in a 34px track.
                            // Slint keeps 32px here; the 2px come from the
                            // narrower rail (theme.sidebarMiniRow).
                            height: root.mini
                                ? (modelData.indent ? 0 : theme.sidebarMiniRow)
                                : 32
                            visible: !(root.mini && modelData.indent)
                            radius: 6
                            // success-bg + success-border while hot. These used
                            // to be the Slint literals "#3fae6a1a" /
                            // "#3fae6a4d" copied verbatim, which is a real
                            // colour bug: Slint's 8-digit form is #RRGGBBAA and
                            // Qt's is #AARRGGBB, so the drop target rendered
                            // olive-brown at 25% alpha instead of success green
                            // at 15% / 35%. The tokens carry the converted
                            // values (theme.successBg = #263fae6a).
                            color: dropHot ? theme.successBg
                                : ((rowArea.containsMouse || isActive) ? theme.surfaceHover : "transparent")
                            border.width: dropHot ? 1 : 0
                            border.color: theme.successBorder

                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: root.mini ? 0 : (modelData.indent ? 24 : 8)
                                anchors.rightMargin: root.mini ? 0 : 6
                                spacing: 8

                                // Leading icon slot: 24px fixed while open (so
                                // every row's name starts at the same x), the
                                // WHOLE row in mini so the lone glyph lands on
                                // the rail's centre line — Slint gets this from
                                // `alignment: mini ? center : stretch`
                                // (Sidebar.slint:230), which a QML Row has no
                                // equivalent for. Without it the slot is the
                                // only visible child, sits at x=0, and the
                                // playlist/folder glyphs render 12px LEFT of
                                // the nav glyphs above them (owner screenshot).
                                // Same idiom as NavRow's slot above.
                                Item {
                                    width: root.mini ? parent.width : 24
                                    height: parent.height
                                    // Folder icon (accent). Every glyph in this
                                    // slot is placed with rounded arithmetic
                                    // (Sidebar.slint:245-246): `centerIn` of an
                                    // odd 15px glyph in an even track lands on a
                                    // half pixel and blurs it.
                                    QbzIcon {
                                        visible: isFolder
                                        name: modelData.expanded && !root.mini ? "folder-open" : "folder"
                                        width: 15
                                        height: 15
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        tintName: "accent"
                                    }
                                    // 2x2 micro-collage (or list-music glyph).
                                    Rectangle {
                                        visible: useCollage
                                        width: 20
                                        height: 20
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        radius: 3
                                        color: theme.surfaceElevated
                                        clip: true
                                        Grid {
                                            anchors.fill: parent
                                            columns: 2
                                            Repeater {
                                                model: modelData.covers.slice(0, 4)
                                                delegate: Image {
                                                    required property string modelData
                                                    width: 10
                                                    height: 10
                                                    source: root.coverMap[modelData] || ""
                                                    fillMode: Image.PreserveAspectCrop
                                                    asynchronous: true
                                                }
                                            }
                                        }
                                    }
                                    QbzIcon {
                                        visible: !isFolder && !useCollage
                                        name: "list-music"
                                        width: 15
                                        height: 15
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        tintName: (rowArea.containsMouse || isActive) ? "textPrimary" : "muted"
                                    }
                                }
                                // Name (hidden in mini). A Row is a positioner
                                // with no stretch, so this width must reserve
                                // EVERY later cell + its spacing or the excess
                                // is pushed past the row's right edge and the
                                // `clip: true` on the Flickable eats it.
                                // (Sidebar.slint gets this free: its
                                // HorizontalLayout stretches the name cell.)
                                // Reserve: 8 + 24 icon slot always; for a
                                // FOLDER also 8 + 14 chevron, plus 8 + the
                                // count label while it is laid out. The old
                                // flat -22 ignored the count, pushing the
                                // chevron ~15px past the clip — which is why
                                // the caret vanished on exactly the folders
                                // that HAVE children (owner report).
                                Text {
                                    visible: !root.mini
                                    width: Math.max(0, parent.width - 32
                                        - (isFolder
                                            ? 22 + (countLabel.visible
                                                ? countLabel.implicitWidth + 8 : 0)
                                            : 0)
                                        // The local mark is a sibling AFTER
                                        // this Text, so its width has to come
                                        // out of the name's reserve or the
                                        // glyph lands past the row's clip.
                                        - (modelData.isLocal === true ? 20 : 0))
                                    height: parent.height
                                    text: modelData.name
                                    color: (rowArea.containsMouse || isActive) ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: 13
                                    font.weight: isFolder ? theme.weightSemibold : theme.weightRegular
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                // LOCAL playlist mark. A local playlist has no
                                // Qobuz side at all — this is how the row says
                                // so, and it is the only visual difference
                                // between the two kinds in the list.
                                QbzIcon {
                                    visible: !root.mini && modelData.isLocal === true
                                    name: "hard-drive"
                                    width: 12
                                    height: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: "muted"
                                }
                                // Folder count (hover) + chevron. The count is
                                // hover-FADED, not hidden — opacity does not
                                // free layout space, so it must stay in the
                                // name's reserve above at all times.
                                Text {
                                    id: countLabel
                                    visible: !root.mini && isFolder && modelData.count > 0
                                    height: parent.height
                                    text: modelData.count
                                    color: theme.textMuted
                                    font.pixelSize: 11
                                    verticalAlignment: Text.AlignVCenter
                                    opacity: rowArea.containsMouse ? 1.0 : 0.0
                                }
                                QbzIcon {
                                    visible: !root.mini && isFolder
                                    name: modelData.expanded ? "chevron-down" : "chevron-right"
                                    width: 14
                                    height: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: rowArea.containsMouse ? "textPrimary" : "muted"
                                }
                            }
                            // Active-row redundant shape cue: 3px accent bar.
                            Rectangle {
                                visible: isActive
                                x: 0
                                width: 3
                                height: parent.height - 12
                                anchors.verticalCenter: parent.verticalCenter
                                radius: 1.5
                                color: theme.accent
                            }
                            MouseArea {
                                id: rowArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                // Right-press anywhere on the row opens its
                                // context menu (Sidebar.slint:389-397). The old
                                // hover "..." button was removed upstream
                                // because the scrollbar overlapped it and ate
                                // the click — do not reintroduce it.
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                // Mini: the row is an icon, so hovering names
                                // it (Sidebar.slint:342-349). No-op while open.
                                onContainsMouseChanged: {
                                    if (containsMouse) root.showMiniTip(plRow, plRow.modelData)
                                    else root.hideMiniTip(plRow.modelData.id)
                                }
                                onClicked: function (mouse) {
                                    // Slint drops the bubble on click so it
                                    // does not linger behind the flyout.
                                    root.hideMiniTip(plRow.modelData.id)
                                    if (mouse.button === Qt.RightButton) {
                                        // SUPPRESSED ON THE RAIL. Slint anchors
                                        // this menu at `row.width - 210px`,
                                        // which on a 34px rail row is deeply
                                        // negative; Qt's viewport clamp would
                                        // silently drop it at x=8, which is not
                                        // the reference's placement, just the
                                        // clamp.
                                        if (root.mini)
                                            return
                                        var lx = plRow.width - rowMenu.menuWidth
                                        if (isFolder)
                                            rowMenu.openForFolder(plRow.modelData, plRow, lx, 28)
                                        else
                                            rowMenu.openForPlaylist(plRow.modelData, plRow, lx, 28)
                                        return
                                    }
                                    if (isFolder) {
                                        // Mini: the nested rows are hidden, so
                                        // a toggle would change NOTHING on
                                        // screen — the contents open in a
                                        // flyout to the right instead
                                        // (Sidebar.slint:355-377).
                                        if (root.mini) root.openFolderFlyout(plRow, plRow.modelData)
                                        else QbzShell.sidebarToggleFolder(modelData.id)
                                    } else {
                                        root.activePlaylistId = modelData.id
                                        QbzBridge.openPlaylist(modelData.id)
                                    }
                                }
                            }
                        }
                    }

                    // Empty state. NOTE: Slint does NOT gate this on mini
                    // (Sidebar.slint:1077-1082) and word-wraps the sentence
                    // inside the 48px track; keeping the gate here is a
                    // deliberate, owner-visible divergence.
                    Text {
                        visible: !root.mini && root.entries.length === 0 && !QbzSession.offline
                        width: plColumn.width
                        text: QbzSession.tr("No playlists yet.", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 12
                        wrapMode: Text.WordWrap
                    }
                }
            }

            // List scrollbar (Sidebar.slint:1090-1137). The shared
            // QbzScrollBar — the port's ListScrollbar replica — rather than a
            // third scrollbar implementation. Two accepted deviations from
            // the sidebar's bespoke Slint one: it reveals on SCROLL (900ms
            // auto-hide) instead of on row-hover, and its thumb is the
            // standard 8/10px instead of the sidebar's thinner 3-6px.
            // Placed in the sidebar's right PADDING (negative margin), like
            // Slint's `x: parent.width - width + (mini ? 8 : Spacing.md)`, so
            // the gutter never overlays the rows and cannot eat their clicks.
            //
            // MINI CAVEAT after the optical re-centring above: the right pad is
            // 4px now, so an 8px gutter pinned to the rail's edge shares its
            // left 4px with the rows. That is fine to LOOK at (an 8px-wide,
            // auto-hiding indicator whose thumb grazes the pill's rounded right
            // edge) but not to CLICK through, so the mini instance is
            // `enabled: false`: indicator only, no drag / click-to-position.
            // Nothing is lost — the wheel scrolls the rail, and a 3px-of-travel
            // drag inside a 50px rail was never a real affordance. The open
            // sidebar keeps the full interactive bar.
            QbzScrollBar {
                target: plFlick
                enabled: !root.mini
                width: root.mini ? 8 : 10
                anchors.right: parent.right
                anchors.rightMargin: -(root.mini ? root.miniPadRight : theme.spacingMd)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
            }
        }
    }
}
