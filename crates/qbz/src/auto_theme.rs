//! Auto-theme controller: wires the Settings "Auto (dynamic)" theme option to
//! `qbz_theme::auto` generation.
//!
//! Generation (DE probing + k-means over the wallpaper/image) runs off the event
//! loop via `spawn_blocking`; the palette is pushed back through
//! `crate::theme::push_colors` (the same path static themes use) on the event
//! loop. On failure the app logs, toasts, and (at startup) falls back to the
//! default OLED theme.
//!
//! Deviation vs Tauri: Tauri regenerated the wallpaper theme reactively; here v1
//! regenerates on activation, on source change, on image pick, and via the
//! explicit "Regenerate" button — there is no live wallpaper file-watcher.

use crate::AppWindow;
use crate::AppearanceState;
use qbz_theme::AutoSource;
use slint::ComponentHandle;

/// Build an [`AutoSource`] from the persisted preferences.
fn source_from_prefs(prefs: &crate::ui_prefs::UiPrefs) -> AutoSource {
    match prefs.auto_theme_source.as_str() {
        "wallpaper" => AutoSource::Wallpaper,
        "image" => AutoSource::Image(prefs.auto_theme_image_path.clone()),
        _ => AutoSource::System,
    }
}

/// Human-readable detected desktop environment (for the Settings "Detected: …"
/// hint row).
pub fn detected_de() -> String {
    qbz_theme::auto::detect_desktop_environment()
        .display_name()
        .to_string()
}

/// Seed the auto-theme Settings state (source index, custom path, detected DE)
/// from the persisted prefs. Called at startup so the controls reflect the saved
/// source when the user opens Settings.
pub fn seed_state(window: &AppWindow) {
    let prefs = crate::ui_prefs::load();
    let state = window.global::<AppearanceState>();
    state.set_auto_theme_source_index(crate::ui_prefs::auto_theme_source_index(
        &prefs.auto_theme_source,
    ));
    state.set_auto_theme_custom_path(prefs.auto_theme_image_path.clone().into());
    state.set_auto_theme_detected_de(detected_de().into());
    state.set_auto_theme_generating(false);
}

/// Synchronous startup apply: generate from the persisted source and push the
/// palette, or fall back to the default (OLED) theme on failure. Runs inline on
/// the event-loop thread during window init so the first paint is already the
/// generated palette.
pub fn apply_startup(window: &AppWindow) {
    let prefs = crate::ui_prefs::load();
    let source = source_from_prefs(&prefs);
    match qbz_theme::generate_auto_theme(&source) {
        Ok(colors) => {
            crate::theme::push_colors(window, &colors, false, false);
            log::info!(
                "[qbz-slint] applied auto theme (source={})",
                prefs.auto_theme_source
            );
        }
        Err(e) => {
            log::warn!(
                "[qbz-slint] auto theme generation failed at startup: {e}; falling back to default"
            );
            crate::theme::apply_theme(window, qbz_theme::default_theme_id());
            crate::toast::error(window, qbz_i18n::t("Auto theme generation failed"));
        }
    }
}

/// Regenerate the auto theme off-thread and push the result on the event loop.
/// Toggles `auto-theme-generating` around the work and toasts on failure.
pub fn regenerate(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    if let Some(w) = weak.upgrade() {
        w.global::<AppearanceState>()
            .set_auto_theme_generating(true);
    }
    handle.spawn(async move {
        let prefs = crate::ui_prefs::load();
        let source = source_from_prefs(&prefs);
        let result = tokio::task::spawn_blocking(move || qbz_theme::generate_auto_theme(&source))
            .await
            .unwrap_or_else(|e| Err(format!("auto theme task panicked: {e}")));

        let _ = weak.upgrade_in_event_loop(move |w| {
            w.global::<AppearanceState>()
                .set_auto_theme_generating(false);
            match result {
                Ok(colors) => crate::theme::push_colors(&w, &colors, false, false),
                Err(e) => {
                    log::warn!("[qbz-slint] auto theme regeneration failed: {e}");
                    crate::toast::error(&w, qbz_i18n::t("Auto theme generation failed"));
                }
            }
        });
    });
}

/// Open the native image picker; on selection persist it as the `image` source
/// and regenerate. Cancel is a no-op (no toast).
pub fn select_image(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let regen_handle = handle.clone();
    handle.spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title(&qbz_i18n::t("Select Image..."))
            .add_filter(
                &qbz_i18n::t("Image"),
                &["png", "jpg", "jpeg", "webp", "bmp", "tiff"],
            )
            .pick_file()
            .await
        else {
            return; // user cancelled
        };
        let path = file.path().to_string_lossy().to_string();

        // Persist source=image + path before regenerating (regenerate re-reads).
        let mut prefs = crate::ui_prefs::load();
        prefs.auto_theme_source = "image".to_string();
        prefs.auto_theme_image_path = path.clone();
        crate::ui_prefs::save(&prefs);

        // Reflect the new source into the Settings controls.
        let ui_path = path.clone();
        let _ = weak.upgrade_in_event_loop(move |w| {
            let st = w.global::<AppearanceState>();
            st.set_auto_theme_custom_path(ui_path.into());
            st.set_auto_theme_source_index(crate::ui_prefs::auto_theme_source_index("image"));
        });

        regenerate(weak, regen_handle);
    });
}

/// Persist a new auto-theme source (from the source dropdown) and regenerate.
pub fn set_source(index: i32, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let key = crate::ui_prefs::auto_theme_source_for_index(index);
    let mut prefs = crate::ui_prefs::load();
    prefs.auto_theme_source = key.to_string();
    crate::ui_prefs::save(&prefs);
    regenerate(weak, handle);
}
