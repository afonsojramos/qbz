//! System tray icon implementation for QBZ
//!
//! Provides system tray integration with playback controls and window management.

use image::GenericImageView;
use std::path::PathBuf;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

// Embed tray icon at compile time (transparent background)
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray.png");

/// Check if running inside Flatpak sandbox
fn is_flatpak() -> bool {
    std::env::var("FLATPAK_ID").is_ok() || std::path::Path::new("/.flatpak-info").exists()
}

/// Ensure tray icon is available in the user's icon theme directory.
/// This makes the icon discoverable by libayatana-appindicator via
/// StatusNotifierItem name lookup on DEs where pixmap data is not supported.
fn ensure_tray_icon_in_theme() {
    let icon_dirs = [
        // Flatpak: /app has icons installed by manifest
        "/app/share/icons/hicolor/32x32/apps/com.blitzfc.qbz.png",
    ];

    // If icon already exists in a known location, nothing to do
    for path in &icon_dirs {
        if std::path::Path::new(path).exists() {
            return;
        }
    }

    // Write embedded tray icon to user's local icon dir so panels can find it
    if let Some(data_dir) = dirs::data_dir() {
        let icon_dir = data_dir.join("icons/hicolor/32x32/apps");
        if std::fs::create_dir_all(&icon_dir).is_ok() {
            let icon_path = icon_dir.join("com.blitzfc.qbz.png");
            if !icon_path.exists() {
                if let Err(e) = std::fs::write(&icon_path, TRAY_ICON_PNG) {
                    log::warn!("Failed to write tray icon to theme dir: {}", e);
                } else {
                    log::info!("Installed tray icon to {:?}", icon_path);
                }
            }
        }
    }
}

/// Get the tray icon - loads from file in Flatpak, embedded data otherwise
fn load_tray_icon() -> Image<'static> {
    // In Flatpak, try to use the installed icon file first
    // This works better with StatusNotifierItem/libayatana-appindicator
    if is_flatpak() {
        let icon_path =
            PathBuf::from("/app/share/icons/hicolor/32x32/apps/com.blitzfc.qbz.png");
        if icon_path.exists() {
            log::info!("Flatpak detected, loading tray icon from: {:?}", icon_path);
            if let Ok(icon_data) = std::fs::read(&icon_path) {
                if let Ok(img) = image::load_from_memory(&icon_data) {
                    let (width, height) = img.dimensions();
                    let rgba = img.into_rgba8().into_raw();
                    return Image::new_owned(rgba, width, height);
                }
            }
            log::warn!("Failed to load icon from path, falling back to embedded");
        }
    }

    // Default: decode embedded PNG
    let img = image::load_from_memory(TRAY_ICON_PNG).expect("Failed to decode tray icon PNG");
    let (width, height) = img.dimensions();
    let rgba = img.into_rgba8().into_raw();
    Image::new_owned(rgba, width, height)
}

/// Initialize the system tray icon with menu
pub fn init_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Initializing system tray icon");

    // Create menu items
    let play_pause = MenuItem::with_id(app, "play_pause", "Play/Pause", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "next", "Next Track", true, None::<&str>)?;
    let previous = MenuItem::with_id(app, "previous", "Previous Track", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let show_hide = MenuItem::with_id(app, "show_hide", "Show/Hide Window", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit QBZ", true, None::<&str>)?;

    // Build tray menu
    let tray_menu = Menu::with_items(
        app,
        &[
            &play_pause,
            &next,
            &previous,
            &separator1,
            &show_hide,
            &separator2,
            &quit,
        ],
    )?;

    // Ensure tray icon is available in icon theme for StatusNotifierItem lookup
    ensure_tray_icon_in_theme();

    // Load custom tray icon (with transparent background)
    let tray_icon = load_tray_icon();

    // Build and display tray icon
    let mut builder = TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&tray_menu)
        .tooltip("QBZ - Music Player")
        .show_menu_on_left_click(false); // Left click toggles window, right click shows menu

    // Set temp dir for icon file so libayatana-appindicator can find it
    // Using XDG_RUNTIME_DIR ensures the host panel can access it in Flatpak
    if let Some(runtime_dir) = dirs::runtime_dir() {
        let tray_dir = runtime_dir.join("qbz-tray");
        if std::fs::create_dir_all(&tray_dir).is_ok() {
            builder = builder.temp_dir_path(&tray_dir);
        }
    }

    let _tray = builder
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            log::info!("Tray menu event: {}", id);

            match id {
                "play_pause" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("tray:play_pause", ());
                    }
                }
                "next" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("tray:next", ());
                    }
                }
                "previous" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("tray:previous", ());
                    }
                }
                "show_hide" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let is_visible = window.is_visible().unwrap_or(false);
                        log::info!("Show/Hide: window visible = {}", is_visible);
                        if is_visible {
                            log::info!("Hiding window");
                            let _ = window.hide();
                        } else {
                            log::info!("Showing window");
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                }
                "quit" => {
                    log::info!("Quit from tray menu");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                // Left click toggles window visibility
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    log::info!("Tray icon left-click");
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        let is_visible = window.is_visible().unwrap_or(true);
                        let is_minimized = window.is_minimized().unwrap_or(false);

                        if is_visible && !is_minimized {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            if is_minimized {
                                let _ = window.unminimize();
                            }
                            let _ = window.set_focus();
                        }
                    }
                }
                // Double click always brings window to front
                TrayIconEvent::DoubleClick { .. } => {
                    log::info!("Tray icon double-click");
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        // Ensure window is visible first
                        let _ = window.show();
                        // Unminimize if minimized
                        if window.is_minimized().unwrap_or(false) {
                            let _ = window.unminimize();
                        }
                        // Always bring to front and focus
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        })
        .build(app)?;

    log::info!("System tray icon initialized");
    Ok(())
}
