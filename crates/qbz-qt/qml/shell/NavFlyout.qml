// Section-nav dropdown flyout — ONE component, mounted by BOTH nav hosts
// (HeaderBar.qml and Sidebar.qml). The QML port of Slint's
// crates/qbz-ui/ui/shell/HeaderMenuOverlay.slint + NavMenuItem.slint, plus
// the entry catalog the Slint tree duplicates three times (Sidebar.slint:724,
// HeaderBar.slint:858 full tabs, HeaderBar.slint:974 compact buttons).
//
// WHY ONE FILE OWNS THE CATALOG TOO: Slint keeps three copies of the same
// [NavMenuEntry] literals because its `if` mount sites cannot share an array
// expression. QML can, so `sections` lives here and both hosts read it —
// there is exactly one place to add a section or an entry.
//
// WHY A Popup: Slint mounts HeaderMenuOverlay as the LAST child of the
// AppShell root because a menu hosted inside the 42px header would be clipped
// by the content area (and a PopupWindow would grab the pointer, killing
// hover-switching between tabs). QtQuick.Controls' Popup reparents its
// content to the window overlay, which gives the same "above everything, not
// clipped by my host" placement WITHOUT touching AppShell.qml — and a
// non-modal Popup does not grab hover, so hover-switching works. The
// closed-sidebar playlists flyout in HeaderBar.qml already uses this pattern.
//
// Open/close behaviour is 1:1 with the Slint overlay:
//   - hovering OR clicking a trigger opens that section's menu;
//   - hovering a different trigger overwrites the open menu (instant switch,
//     single-open is automatic — there is one panel);
//   - the menu closes after 1s idle, and the countdown is PAUSED while the
//     pointer rests on the trigger or anywhere on the panel (panel body OR a
//     row — hence the two-flag OR, HeaderMenuOverlay.slint:41);
//   - a press outside, or Escape, closes it.
//
// GAP vs Slint: no drop shadow. Qt's shadow effects (Qt5Compat /
// QtQuick.Effects MultiEffect) render NOTHING on the software path this port
// runs on (see RoundedImage.qml / TrackInfoModal.qml) — the 1px border-muted
// outline carries the separation instead.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Item {
    id: nav
    // Zero-size, non-layout host: it exists only to give the Popup a
    // coordinate system (the host root's) and to carry the shared catalog.
    // Mount it as a DIRECT child of the host root, never inside a Row/Column,
    // so its origin stays the host root's origin.
    width: 0
    height: 0

    // ---------------------------------------------------------------------
    // Catalog
    // ---------------------------------------------------------------------

    // Slint gates the Recommendations entry on SettingsState.show-recommendations
    // (discover_prefs `show_recommendations`, default ON) — the port publishes
    // it in QbzBridge.settingsJson.
    readonly property bool showRecommendations: {
        try {
            return JSON.parse(QbzBridge.settingsJson).showRecommendations !== false
        } catch (e) {
            return true
        }
    }

    // The four top-level sections, in Slint order. `rev` is threaded through
    // buildSections purely so the binding re-runs on a language switch
    // (QbzSession.trRev is the translation revision counter every other
    // tr() call site binds to).
    readonly property var sections: buildSections(QbzSession.trRev, showRecommendations)

    function trs(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // Entry shape: { label, icon (baked glyph name, "" = no glyph),
    //                view (shell route), tab (view-internal tab id),
    //                enabled }
    // Section shape: { id, icon, label, qobuz (hidden while offline),
    //                  enabled, entries }
    function buildSections(rev, reco) {
        // Discover — glyph-less rows (Slint has-glyph: false).
        var discover = [
            { "label": nav.trs("Home"), "icon": "", "view": "home", "tab": "home", "enabled": true },
            { "label": nav.trs("Editor's Picks"), "icon": "", "view": "home", "tab": "editorPicks", "enabled": true },
            { "label": nav.trs("For You"), "icon": "", "view": "home", "tab": "forYou", "enabled": true }
        ]
        if (reco) {
            discover.push({ "label": nav.trs("Recommendations"), "icon": "", "view": "home", "tab": "recommendations", "enabled": true })
        }
        return [
            {
                "id": "discover",
                "icon": "compass",
                "label": nav.trs("Discover"),
                "qobuz": true,
                "enabled": true,
                "entries": discover
            },
            {
                "id": "library",
                "icon": "music-library-2",
                "label": nav.trs("Library"),
                "qobuz": true,
                "enabled": true,
                "entries": [
                    { "label": nav.trs("All"), "icon": "layout-grid", "view": "library", "tab": "all", "enabled": true },
                    { "label": nav.trs("Tracks"), "icon": "music", "view": "library", "tab": "tracks", "enabled": true },
                    { "label": nav.trs("Albums"), "icon": "disc", "view": "library", "tab": "albums", "enabled": true },
                    { "label": nav.trs("Artists"), "icon": "user", "view": "library", "tab": "artists", "enabled": true },
                    { "label": nav.trs("Playlists"), "icon": "list-music", "view": "library", "tab": "playlists", "enabled": true },
                    { "label": nav.trs("Labels"), "icon": "disc-3", "view": "library", "tab": "labels", "enabled": true }
                ]
            },
            {
                "id": "local",
                "icon": "hard-drive",
                "label": nav.trs("Local Library"),
                "qobuz": false,
                "enabled": true,
                "entries": [
                    { "label": nav.trs("Albums"), "icon": "disc", "view": "local", "tab": "albums", "enabled": true },
                    { "label": nav.trs("Artists"), "icon": "user", "view": "local", "tab": "artists", "enabled": true },
                    { "label": nav.trs("Folders"), "icon": "folder", "view": "local", "tab": "folders", "enabled": true },
                    { "label": nav.trs("Tracks"), "icon": "music", "view": "local", "tab": "tracks", "enabled": true }
                ]
            },
            {
                // GAP: this port has NO My QBZ surface — the shell router maps
                // home / library / local / localalbum / album / artist /
                // settings / search / playlist, and neither Collections nor
                // Mixtapes has a view behind it. The whole section is disabled
                // (dimmed, pointer-inert, no flyout) rather than opening a menu
                // whose every row clicks into nothing. The entries are kept
                // here so the port stays legible against Slint.
                "id": "myqbz",
                "icon": "qbz-symbolic",
                // Slint: MyQbzBrandingState.label (user-editable, defaults to
                // "My QBZ"); this port has no branding state yet.
                "label": nav.trs("My QBZ"),
                "qobuz": false,
                "enabled": false,
                "entries": [
                    { "label": nav.trs("Collections"), "icon": "library-big", "view": "", "tab": "", "enabled": false },
                    { "label": nav.trs("Mixtapes"), "icon": "cassette-tape", "view": "", "tab": "", "enabled": false }
                ]
            }
        ]
    }

    // Which section owns the highlight, derived from the LIVE view (both hosts
    // used to carry this identical binding). Slint highlights off
    // HeaderMenuState.open-index; the hosts OR that in via `openId`.
    readonly property string activeSection:
          QbzShell.currentView === "home" ? "discover"
        : QbzShell.currentView === "library" ? "library"
        : (QbzShell.currentView === "local" || QbzShell.currentView === "localalbum") ? "local"
        : ""

    // ---------------------------------------------------------------------
    // Open / close (HeaderMenuState)
    // ---------------------------------------------------------------------

    // Section id whose menu is open ("" = none) — the port's open-index.
    // Keyed on `visible` (flips the instant open()/close() runs), not on
    // `opened` (which waits out an enter transition).
    readonly property string openId: panel.visible ? panel.sectionId : ""
    // True while the pointer rests on the TRIGGER. The trigger owns this flag
    // (HeaderMenuOverlay.slint:51): only the trigger that owns the open menu
    // clears it, which avoids a leave/enter race when sliding between triggers.
    property bool triggerHovered: false

    // Header dropdowns hang at a fixed window y (Slint: 45px = the 42px header
    // plus 3), NOT under each tab — the host mounts this at its own origin, so
    // in HeaderBar (top of the window) the two coincide.
    property real dropdownY: 45

    // Header form: menu under the trigger, left edges aligned.
    // `withTitle` = the section-name header row + hairline, which Slint shows
    // for the compact icon buttons (the icon alone does not name the section)
    // and omits for the full text tabs.
    function openUnder(trigger, section, withTitle) {
        if (!section || section.enabled === false) return
        panel.show(section, withTitle === true, trigger.mapToItem(nav, 0, 0).x, nav.dropdownY)
    }

    // Sidebar form: menu to the RIGHT of the row (+6px), aligned to its top,
    // always headed by the section name (Sidebar.slint:590).
    function openBeside(trigger, section) {
        if (!section || section.enabled === false) return
        var p = trigger.mapToItem(nav, trigger.width + 6, 0)
        panel.show(section, true, p.x, p.y)
    }

    function closeMenu() { panel.close() }

    // ---------------------------------------------------------------------
    // Entry activation
    // ---------------------------------------------------------------------

    // The tab a just-navigated view must land on, held until the view mounts.
    property string pendingTab: ""
    property string pendingView: ""

    function activate(entry) {
        panel.close()
        if (!entry || entry.enabled === false || entry.view === "") return
        if (QbzShell.currentView === entry.view) {
            // Already mounted — no currentViewChanged will fire.
            applyTab(entry.tab)
            return
        }
        nav.pendingTab = entry.tab
        nav.pendingView = entry.view
        QbzShell.navigateTo(entry.view)
    }

    Connections {
        target: QbzShell
        function onCurrentViewChanged() {
            if (nav.pendingTab === "") return
            var wanted = nav.pendingView
            var tab = nav.pendingTab
            nav.pendingTab = ""
            nav.pendingView = ""
            // A navigation somewhere else (back/forward, a card click) beat us
            // to it — drop the request instead of stamping a stale tab later.
            if (QbzShell.currentView !== wanted) return
            // The content Loader instantiates the view from its own binding on
            // the same property; binding order between it and this handler is
            // undefined, so apply after the current pass.
            Qt.callLater(function () { nav.applyTab(tab) })
        }
    }

    // Selects a tab INSIDE the mounted view.
    //
    // TEMPORARY BRIDGE (the permanent fix is in the handoff report): the shell
    // router only carries a view id, and the three tabbed views own their tab
    // as plain QML state (HomeView/LibraryView/LocalLibraryView `activeTab`).
    // There is no bridge property to hand a tab through yet, so the mounted
    // view is located in the scene and its `activeTab` assigned directly. The
    // search is depth-capped (the view sits 5 levels under the window content
    // item: screenLoader > AppShell > contentFrame > Loader > view), so it
    // never descends into delegate forests, and it degrades to a plain
    // section navigation if nothing is found.
    function applyTab(tab) {
        if (!tab || tab === "") return
        var top = nav
        while (top.parent) top = top.parent
        var view = findTabHost(top, 0)
        if (view) view.activeTab = tab
    }

    function findTabHost(item, depth) {
        if (!item || depth > 7) return null
        // Only HomeView / LibraryView / LocalLibraryView expose `activeTab`,
        // and exactly one of them is mounted at a time (one content Loader).
        if (typeof item.activeTab === "string") return item
        var kids = item.children
        if (!kids) return null
        for (var i = 0; i < kids.length; i++) {
            var hit = findTabHost(kids[i], depth + 1)
            if (hit) return hit
        }
        return null
    }

    QbzTheme { id: theme }

    // Idle-close (project rule: 1s), paused while the pointer rests on the
    // menu or on its trigger. Lives HERE and not inside the Popup: the Popup
    // carries an explicit contentItem, so a non-visual default-property child
    // would be handed to that content item instead of the popup.
    Timer {
        interval: 1000
        repeat: false
        running: panel.visible && !panel.menuHovered
        onTriggered: panel.close()
    }

    // ---------------------------------------------------------------------
    // The panel (HeaderMenuOverlay's Rectangle + NavMenuItem rows)
    // ---------------------------------------------------------------------

    // One menu row — NavMenuItem.slint: 32px, hover surface-hover, 14px glyph
    // (optional), 13px text-secondary label, padding 14 left / 22 right.
    component NavMenuRow: Rectangle {
        id: row
        property var entry: null
        property int rowIndex: -1
        readonly property bool rowEnabled: entry && entry.enabled !== false

        width: parent ? parent.width : 0
        height: 32
        opacity: rowEnabled ? 1.0 : 0.45
        color: (rowEnabled && rowArea.containsMouse) ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 14
            anchors.rightMargin: 22
            spacing: 9

            QbzIcon {
                visible: row.entry && row.entry.icon !== ""
                width: visible ? 14 : 0
                height: 14
                anchors.verticalCenter: parent.verticalCenter
                name: row.entry ? row.entry.icon : ""
                tintName: "secondary"
            }
            Text {
                height: parent.height
                width: parent.width - (row.entry && row.entry.icon !== "" ? 23 : 0)
                text: row.entry ? row.entry.label : ""
                color: theme.textSecondary
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }

        MouseArea {
            id: rowArea
            anchors.fill: parent
            enabled: row.rowEnabled
            hoverEnabled: row.rowEnabled
            cursorShape: Qt.PointingHandCursor
            onClicked: nav.activate(row.entry)
            // Per-row hover keeps the idle timer paused while the cursor RESTS
            // on a row (the panel-body tracker below never sees it — the row
            // is on top of it).
            onContainsMouseChanged: {
                if (containsMouse) panel.hoveredRow = row.rowIndex
                else if (panel.hoveredRow === row.rowIndex) panel.hoveredRow = -1
            }
        }
    }

    Popup {
        id: panel
        width: 188
        padding: 0
        topPadding: 6
        bottomPadding: 6
        // Deterministic height (Slint: panel-box.preferred-height) — 32px per
        // row, plus the optional 26px title band and its 1px hairline.
        height: 12 + (panel.title === "" ? 0 : 27) + panel.entries.length * 32
        closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

        property string sectionId: ""
        property string title: ""
        property var entries: []
        // Which row the pointer is on (-1 = none); rows do not overlap.
        property int hoveredRow: -1
        // True while the pointer rests on the panel BODY (padding/gaps).
        property bool bodyHovered: false
        // Pointer anywhere on the menu, or on the trigger that owns it — the
        // idle countdown is paused while this holds.
        readonly property bool menuHovered:
            bodyHovered || hoveredRow >= 0 || nav.triggerHovered

        function show(section, withTitle, px, py) {
            // Switching sections: drop stale hover state (the new rows have
            // not reported yet), exactly like the Slint `changed open-index`.
            if (panel.sectionId !== section.id) panel.hoveredRow = -1
            panel.sectionId = section.id
            panel.title = withTitle ? section.label : ""
            panel.entries = section.entries
            panel.x = px
            panel.y = py
            if (!panel.visible) panel.open()
        }

        onClosed: {
            panel.sectionId = ""
            panel.hoveredRow = -1
            panel.bodyHovered = false
            nav.triggerHovered = false
        }

        background: Rectangle {
            color: theme.surfaceMain
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderMuted

            // Panel-body hover tracker. It lives in the BACKGROUND, i.e. below
            // the content item, so it can never swallow a row's press (a fill
            // MouseArea declared over the rows would — a press is not a
            // composed event and propagateComposedEvents does not rescue it).
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                onContainsMouseChanged: panel.bodyHovered = containsMouse
                // Swallow clicks on the panel chrome so they do not fall
                // through to whatever sits under the flyout.
                onClicked: {}
            }
        }

        contentItem: Column {
            spacing: 0

            // Optional section-name header + hairline (Slint: shown for the
            // compact header buttons and every sidebar flyout).
            Text {
                visible: panel.title !== ""
                width: parent.width
                height: visible ? 26 : 0
                leftPadding: 14
                rightPadding: 14
                text: panel.title
                color: theme.textMuted
                font.pixelSize: 11
                font.weight: theme.weightSemibold
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Rectangle {
                visible: panel.title !== ""
                width: parent.width
                height: visible ? 1 : 0
                color: theme.borderSubtle
            }

            Repeater {
                model: panel.entries
                delegate: NavMenuRow {
                    required property var modelData
                    required property int index
                    entry: modelData
                    rowIndex: index
                }
            }
        }
    }
}
