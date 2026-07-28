// Settings > Appearance (phase 19) — the QML port of
// crates/qbz-ui/ui/settings/AppearanceSettings.slint. Row order, group
// headers and gating mirror the Slint 1:1; every control rides the single
// settingsJson document (root.doc) and the settingsBool/Select/String
// invokables — never local truth.
//
// The THEME row is live: QbzShell.themeSlug / themeListJson / themeFilter
// (theme_qt.rs) and themeSet() repaints the whole app through QbzTheme.qml
// (the Slint theme::push_colors equivalent).
//
// POC-NOTEs (deliberate cuts):
// - "Auto (dynamic)" rows: the Source dropdown persists, Regenerate
//   re-resolves via AutoSource::System; the "Select Image..." flow
//   (file dialog + AutoSource::Image) is not ported.
// - "Custom" rows: the Slint token editor is not ported (owner: read-only
//   apply of custom_theme.json) — an info row stands in.
// - The "system" registry theme maps to the Dark palette (Slint reads the
//   OS palette).
// - Language persists but applies on next launch (live switch = phase 20).
// - UI scale / renderer / preferred GPU persist with restart semantics;
//   the Slint restart toasts are logs here.
// - Preferred GPU: no adapter enumeration — Auto + the persisted adapter.
// - The commented-out Slint blocks (immersive background/panels, title
//   template) stay out, 1:1 the owner's cut.

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
            // Filter cycle (All -> Dark -> Light): sun-moon / moon / sun.
            Rectangle {
                width: 34
                height: 34
                radius: theme.radiusSm
                border.width: 1
                border.color: theme.borderSubtle
                color: fArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                QbzIcon {
                    anchors.centerIn: parent
                    name: QbzShell.themeFilter === 1 ? "moon"
                        : QbzShell.themeFilter === 2 ? "sun" : "sun-moon"
                    width: 16
                    height: 16
                    tintName: "secondary"
                }
                MouseArea {
                    id: fArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzShell.themeSetFilter((QbzShell.themeFilter + 1) % 3)
                }
            }
            QbzSelect {
                menuWidth: 220
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
        QbzSelect {
            menuWidth: 200
            options: root.doc.appBackgroundModes || []
            currentIndex: root.doc.appBackgroundIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("app-background", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Intelligent Search", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.intelligentSearch === true
            onToggled: function (v) { QbzBridge.settingsBool("intelligent-search", v) }
        }
    }
    // Auto-theme rows (theme slug "auto" only).
    SettingRow {
        visible: QbzShell.themeSlug === "auto"
        label: QbzSession.tr("Source", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: [QbzSession.tr("System Colors", QbzSession.trRev), QbzSession.tr("Wallpaper Sync", QbzSession.trRev), QbzSession.tr("Custom Image", QbzSession.trRev)]
            currentIndex: root.doc.autoThemeSourceIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("auto-theme-source", i) }
        }
    }
    SettingRow {
        visible: QbzShell.themeSlug === "auto"
        label: QbzSession.tr("Regenerate", QbzSession.trRev)
        description: QbzSession.tr("Rebuild the palette from the source (System colors in the POC).", QbzSession.trRev)
        SettingsButton {
            text: QbzSession.tr("Regenerate", QbzSession.trRev)
            onClicked: QbzShell.themeSet(QbzShell.themeSlug)
        }
    }
    // Custom-theme rows (theme slug "custom" only) — no token editor.
    SettingRow {
        visible: QbzShell.themeSlug === "custom"
        label: QbzSession.tr("Custom theme", QbzSession.trRev)
        description: QbzSession.tr("Applied read-only from custom_theme.json — the token editor is not ported to the Qt frontend (POC).", QbzSession.trRev)
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
        description: QbzSession.tr("Restart to apply.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
            options: root.doc.uiScales || []
            currentIndex: root.doc.uiScaleIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("ui-scale", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Immersive search", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.immersiveSearchActions || []
            currentIndex: root.doc.immersiveSearchActionIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("immersive-search-action", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Immersive default view", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
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
        QbzToggle {
            checked: root.doc.navInSidebar === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-in-sidebar", v) }
        }
    }
    SettingRow {
        // ADR-010: only mounted when navigation is NOT in the sidebar.
        visible: root.doc.navInSidebar !== true
        label: QbzSession.tr("Compact header navigation", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.navHeaderCompact === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-header-compact", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("My QBZ", QbzSession.trRev)
        Row {
            spacing: 8
            RoundedImage {
                width: 34
                height: 34
                source: "../assets/qbz-logo.png"
                radius: theme.radiusSm
            }
            QbzLineEdit {
                width: 150
                text: root.doc.myQbzLabel || ""
                placeholder: QbzSession.tr("My QBZ", QbzSession.trRev)
                onCommitted: function (v) { QbzBridge.settingsString("myqbz-label", v) }
            }
            SettingsButton {
                iconName: "rotate-ccw"
                onClicked: QbzBridge.settingsString("myqbz-label", "")
            }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show playlist cover collages in sidebar", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.sidebarPlaylistCollage === true
            onToggled: function (v) { QbzBridge.settingsBool("sidebar-playlist-collage", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show album artwork in Local Library track list", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.localLibraryTrackArtwork === true
            onToggled: function (v) { QbzBridge.settingsBool("local-library-track-artwork", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Play indicator animation", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.playIndicatorAnimation === true
            onToggled: function (v) { QbzBridge.settingsBool("play-indicator-animation", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Invert swipe navigation direction", QbzSession.trRev)
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
        description: QbzSession.tr("Restart to apply.", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.useSystemTitleBar === true
            onToggled: function (v) { QbzBridge.settingsBool("use-system-title-bar", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Hide title bar", QbzSession.trRev)
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
        QbzToggle {
            checked: root.doc.showVolumeSteppers === true
            onToggled: function (v) { QbzBridge.settingsBool("show-volume-steppers", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Mini player default view", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.miniDefaultViews || []
            currentIndex: root.doc.miniDefaultViewIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("mini-default-view", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Startup page", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.startupPages || []
            currentIndex: root.doc.startupPageIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("startup-page", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Show Purchases", QbzSession.trRev)
        QbzToggle {
            checked: root.doc.showPurchases === true
            onToggled: function (v) { QbzBridge.settingsBool("show-purchases", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Purchases in title bar", QbzSession.trRev)
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
        QbzToggle {
            checked: root.doc.trayEnable === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-enable", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Close to tray", QbzSession.trRev)
        rowEnabled: root.doc.trayEnable === true
        QbzToggle {
            enabled: root.doc.trayEnable === true
            checked: root.doc.trayCloseToTray === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-close-to-tray", v) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Tray icon variant", QbzSession.trRev)
        QbzSelect {
            menuWidth: 200
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
        description: QbzSession.tr("Restart to apply.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 220
            options: root.doc.renderers || []
            currentIndex: root.doc.rendererIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("renderer", i) }
        }
    }
    SettingRow {
        label: QbzSession.tr("Preferred GPU", QbzSession.trRev)
        description: QbzSession.tr("Restart to apply.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 260
            options: root.doc.gpuPowers || []
            currentIndex: root.doc.gpuPowerIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("gpu-power", i) }
        }
    }
}
