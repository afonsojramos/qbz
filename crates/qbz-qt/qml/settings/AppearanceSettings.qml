// Settings > Appearance — the QML port of crates/qbz-ui/ui/settings/
// AppearanceSettings.slint. Group order, row order, labels, descriptions and
// gating are 1:1 with the Slint; every control rides the settingsJson
// document (root.doc) and the settingsBool/Select/String invokables — never
// local truth.
//
// The THEME row is live: QbzShell.themeSlug / themeListJson / themeFilter
// (theme_qt.rs) and themeSet() repaints the whole app through QbzTheme.qml
// (the Slint theme::push_colors equivalent).
//
// Not shipped (no backend seam in this port):
// - The "Auto (dynamic)" custom-image picker ("Select Image..." + the
//   "Detected: <desktop>" line): there is no file-chooser seam here, so the
//   button would do nothing. The Source dropdown still persists all three
//   values (the shared ui_prefs key the Slint reads).
// - The Custom-theme editor (seed-from-current, the dark-polarity toggle and
//   the base-token swatch grid): the port applies custom_theme.json read-only
//   and has no ColorPicker primitive.
// - "Hide Dock icon when closed to menu bar" (macOS-only in the Slint too).
// - The commented-out Slint blocks (immersive background / panels / FPS,
//   window-title template) stay out, 1:1 the owner's cut.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // --- Theme row state (theme_qt.rs via the bridge) --------------------
    readonly property var themeEntries: {
        try {
            return JSON.parse(QbzShell.themeListJson)
        } catch (e) {
            return []
        }
    }
    // 0 All / 1 Dark / 2 Light; the Auto/Custom synthetics only list under
    // All (theme.rs:217-227).
    readonly property var filteredThemes: {
        const f = QbzShell.themeFilter
        const out = []
        for (let i = 0; i < themeEntries.length; i++) {
            const e = themeEntries[i]
            const synthetic = e.slug === "auto" || e.slug === "custom"
            if (f === 1 && (e.isLight || synthetic)) continue
            if (f === 2 && (!e.isLight || synthetic)) continue
            out.push(e)
        }
        return out
    }
    readonly property var filteredLabels: filteredThemes.map(function (e) { return e.label })
    readonly property int currentThemeIndex: {
        for (let i = 0; i < filteredThemes.length; i++) {
            if (filteredThemes[i].slug === QbzShell.themeSlug) return i
        }
        return -1
    }
    readonly property bool tbLocked: doc.hideTitleBar === true || doc.useSystemTitleBar === true

    // ============================ THEME ==================================
    GroupHeader { text: QbzSession.tr("THEME", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Theme", QbzSession.trRev)
        Row {
            spacing: 8
            // Theme list filter cycle (All / Dark / Light).
            SettingsButton {
                iconName: QbzShell.themeFilter === 1 ? "moon"
                    : QbzShell.themeFilter === 2 ? "sun" : "sun-moon"
                onClicked: QbzShell.themeSetFilter((QbzShell.themeFilter + 1) % 3)
            }
            QbzSelect {
                menuWidth: 220
                // 36 registered themes — a name filter (Slint parity).
                searchable: true
                options: root.filteredLabels
                currentIndex: root.currentThemeIndex
                onSelected: function (i) {
                    if (i >= 0 && i < root.filteredThemes.length)
                        QbzShell.themeSet(root.filteredThemes[i].slug)
                }
            }
        }
    }
    SettingRow {
        label: QbzSession.tr("Album header gradient", QbzSession.trRev)
        description: QbzSession.tr("Use artwork-derived blur as a backdrop in album and artist detail views.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.albumHeaderGradient === true
            onToggled: function (v) { QbzBridge.settingsBool("album-header-gradient", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Dynamic background", QbzSession.trRev)
        description: QbzSession.tr("Animated album-art background behind the whole app. High resource use — GPU accelerated.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.appBackgroundModes || []
            currentIndex: root.doc.appBackgroundIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("app-background", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Intelligent Search", QbzSession.trRev)
        description: QbzSession.tr("Smart search cache, ranking, and the search preview dropdown. On by default.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.intelligentSearch === true
            onToggled: function (v) { QbzBridge.settingsBool("intelligent-search", v) }
        }
    }
    // Auto-theme rows (the "auto" theme only).
    SettingRow {
        visible: QbzShell.themeSlug === "auto"
        label: QbzSession.tr("Source", QbzSession.trRev)
        description: QbzSession.tr("Generate a color theme from your system wallpaper or a custom image", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.autoThemeSources || []
            currentIndex: root.doc.autoThemeSourceIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("auto-theme-source", i) }
        }
    }
    SettingRow {
        visible: QbzShell.themeSlug === "auto"
        label: QbzSession.tr("Regenerate", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Regenerate", QbzSession.trRev)
            onClicked: QbzShell.themeSet(QbzShell.themeSlug)
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ===================== TYPOGRAPHY & LANGUAGE =========================
    GroupHeader { text: QbzSession.tr("TYPOGRAPHY & LANGUAGE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Language", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.languages || []
            currentIndex: root.doc.languageIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("language", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Interface size", QbzSession.trRev)
        description: QbzSession.tr("Scales the whole interface, like browser zoom. Small fits more content on screen, Large and Extra large improve readability (requires restart)", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.uiScales || []
            currentIndex: root.doc.uiScaleIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("ui-scale", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Immersive search", QbzSession.trRev)
        description: QbzSession.tr("What selecting a result in the Immersive search does. Disabled turns the in-immersive search off.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.immersiveSearchActions || []
            currentIndex: root.doc.immersiveSearchActionIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("immersive-search-action", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Immersive default view", QbzSession.trRev)
        description: QbzSession.tr("Which immersive view opens by default. 'Remember last' restores your last view.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.immersiveDefaultViews || []
            currentIndex: root.doc.immersiveDefaultViewIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("immersive-default-view", i) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ======================= LIBRARY & VISUALS ===========================
    GroupHeader { text: QbzSession.tr("LIBRARY & VISUALS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Show navigation in sidebar", QbzSession.trRev)
        description: QbzSession.tr("Move the Discover, Library, Local Library and My QBZ sections out of the header and into the sidebar.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.navInSidebar === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-in-sidebar", v) }
        }
    }
    SettingRow {
        // ADR-010: only mounted when navigation is NOT in the sidebar.
        visible: root.doc.navInSidebar !== true
        label: QbzSession.tr("Compact header navigation", QbzSession.trRev)
        description: QbzSession.tr("Use the icon-only section navigation in the header even while the sidebar is open.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.navHeaderCompact === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-header-compact", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("My QBZ", QbzSession.trRev)
        description: QbzSession.tr("Rename the My QBZ hub. Leave the name blank (or hit reset) to restore the default.", QbzSession.trRev)
        Row {
            spacing: 8
            // Fixed icon preview (RENAME ONLY — no custom icon, owner DQ3).
            RoundedImage {
                width: 34
                height: 34
                anchors.verticalCenter: parent.verticalCenter
                source: "../assets/qbz-logo.png"
                radius: theme.radiusSm
            }
            QbzLineEdit {
                width: 150
                anchors.verticalCenter: parent.verticalCenter
                text: root.doc.myQbzLabel || ""
                placeholder: QbzSession.tr("My QBZ", QbzSession.trRev)
                onCommitted: function (v) { QbzBridge.settingsString("myqbz-label", v) }
            }
            SettingsButton {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "rotate-ccw"
                onClicked: QbzBridge.settingsString("myqbz-label", "")
            }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show playlist cover collages in sidebar", QbzSession.trRev)
        description: QbzSession.tr("Render a 2×2 thumbnail of track covers next to each playlist. Disable on low-end machines to skip the extra images.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.sidebarPlaylistCollage === true
            onToggled: function (v) { QbzBridge.settingsBool("sidebar-playlist-collage", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show album artwork in Local Library track list", QbzSession.trRev)
        description: QbzSession.tr("Display the album cover thumbnail next to each track in the Tracks and Folders views. Off by default — large libraries pay a per-row image-decode cost.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.localLibraryTrackArtwork === true
            onToggled: function (v) { QbzBridge.settingsBool("local-library-track-artwork", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Play indicator animation", QbzSession.trRev)
        description: QbzSession.tr("Animate the now-playing row with equalizer bars. Off (default) shows a static pause icon with an accent edge mark — lighter on CPU.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.playIndicatorAnimation === true
            onToggled: function (v) { QbzBridge.settingsBool("play-indicator-animation", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Invert swipe navigation direction", QbzSession.trRev)
        description: QbzSession.tr("Swap the two-finger touchpad swipe: left goes back, right goes forward.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.invertSwipeNavigation === true
            onToggled: function (v) { QbzBridge.settingsBool("invert-swipe-navigation", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= NOTIFICATIONS =============================
    GroupHeader { text: QbzSession.tr("NOTIFICATIONS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("In-app toasts notifications", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.inAppToasts === true
            onToggled: function (v) { QbzBridge.settingsBool("in-app-toasts", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("System Notifications", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.systemNotifications === true
            onToggled: function (v) { QbzBridge.settingsBool("system-notifications", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= WINDOW TITLE ==============================
    GroupHeader { text: QbzSession.tr("WINDOW TITLE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Show track in window title", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.windowTitleShow === true
            onToggled: function (v) { QbzBridge.settingsBool("window-title-show", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================== TITLE BAR ================================
    GroupHeader { text: QbzSession.tr("TITLE BAR", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Use system title bar", QbzSession.trRev)
        description: QbzSession.tr("Keep your system's native window decorations. Turn off to use the QBZ header as the title bar, with its own window controls and drag support. Takes effect after restarting QBZ.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.useSystemTitleBar === true
            onToggled: function (v) { QbzBridge.settingsBool("use-system-title-bar", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Hide title bar", QbzSession.trRev)
        description: QbzSession.tr("Frameless window without window controls or header drag (for tiling window manager users)", QbzSession.trRev)
        rowEnabled: root.doc.useSystemTitleBar !== true
        QbzToggle {
            enabled: root.doc.useSystemTitleBar !== true
            checked: root.doc.hideTitleBar === true
            onToggled: function (v) { QbzBridge.settingsBool("hide-title-bar", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ======================= WINDOW CONTROLS =============================
    GroupHeader { text: QbzSession.tr("WINDOW CONTROLS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Window controls position", QbzSession.trRev)
        description: QbzSession.tr("Place the window control buttons on the left or right side of the title bar", QbzSession.trRev)
        rowEnabled: !root.tbLocked
        QbzSelect {
            enabled: !root.tbLocked
            menuWidth: 160
            options: root.doc.wcPositions || []
            currentIndex: root.doc.wcPositionIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("wc-position", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show window controls", QbzSession.trRev)
        description: QbzSession.tr("Show minimize, maximize, and close buttons in the title bar. Disable if your window manager handles these.", QbzSession.trRev)
        rowEnabled: !root.tbLocked
        QbzToggle {
            enabled: !root.tbLocked
            checked: root.doc.showWindowControls === true
            onToggled: function (v) { QbzBridge.settingsBool("show-window-controls", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= PLAYER VIEWS ==============================
    GroupHeader { text: QbzSession.tr("PLAYER VIEWS", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Show volume +/- buttons", QbzSession.trRev)
        description: QbzSession.tr("Add discrete plus and minus buttons next to the volume slider in the player bar.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.showVolumeSteppers === true
            onToggled: function (v) { QbzBridge.settingsBool("show-volume-steppers", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Mini player default view", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.miniDefaultViews || []
            currentIndex: root.doc.miniDefaultViewIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("mini-default-view", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Startup page", QbzSession.trRev)
        description: QbzSession.tr("Choose which page to show when the app starts", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.startupPages || []
            currentIndex: root.doc.startupPageIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("startup-page", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show Purchases", QbzSession.trRev)
        description: QbzSession.tr("Show the Purchases section in the sidebar for browsing and downloading your purchased music", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.showPurchases === true
            onToggled: function (v) { QbzBridge.settingsBool("show-purchases", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Purchases in title bar", QbzSession.trRev)
        description: QbzSession.tr("Place the Purchases entry in the custom title bar instead of the sidebar", QbzSession.trRev)
        rowEnabled: !root.tbLocked
        QbzToggle {
            enabled: !root.tbLocked
            checked: root.doc.navTbPurchases === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-tb-purchases", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================== SYSTEM TRAY ==============================
    GroupHeader { text: QbzSession.tr("SYSTEM TRAY", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Enable tray icon", QbzSession.trRev)
        description: QbzSession.tr("Show icon in system tray (requires restart)", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.trayEnable === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-enable", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Close to tray", QbzSession.trRev)
        description: QbzSession.tr("Keep playing in the tray instead of quitting when you close the window", QbzSession.trRev)
        rowEnabled: root.doc.trayEnable === true
        QbzToggle {
            enabled: root.doc.trayEnable === true
            checked: root.doc.trayCloseToTray === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-close-to-tray", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Tray icon variant", QbzSession.trRev)
        description: QbzSession.tr("Pick a mono glyph to match your panel (Plasma, GNOME's permanently dark top bar) or the full colour vinyl logo", QbzSession.trRev)
        QbzSelect {
            menuWidth: 160
            options: root.doc.trayIconThemes || []
            currentIndex: root.doc.trayIconThemeIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("tray-icon-theme", i) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // =========================== RENDERER ================================
    GroupHeader { text: QbzSession.tr("RENDERER", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Rendering backend", QbzSession.trRev)
        description: QbzSession.tr("Auto picks the best renderer for your graphics hardware. Only change this if the app feels slow or renders incorrectly (requires restart)", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.renderers || []
            currentIndex: root.doc.rendererIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("renderer", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Preferred GPU", QbzSession.trRev)
        description: QbzSession.tr("Which GPU renders the app. On a hybrid laptop, picking the discrete GPU moves the dynamic-background load off the integrated one (cooler, but more power). Auto is recommended (requires restart)", QbzSession.trRev)
        QbzSelect {
            menuWidth: 260
            options: root.doc.gpuPowers || []
            currentIndex: root.doc.gpuPowerIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("gpu-power", i) }
        }
    }

    Item { width: 1; height: 40 }
}
