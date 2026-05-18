//! QBZ Slint POC binary.
//!
//! Validates whether Slint can become QBZ's future UI foundation on top of
//! the framework-agnostic `qbz-app` / `qbz-core` stack. See the POC ADR
//! (`qbz-nix-docs/qbz-adr/qbz_slint_functional_poc_adr.md`).
//!
//! Lives only on the private `slint-poc` branch (ADR-007). The Slint UI tree
//! is compiled from `ui/app.slint` by `build.rs`; `include_modules!` pulls in
//! the generated Rust bindings.
//!
//! M1 status: foundation tokens, login screen, and the app shell frame
//! (sidebar / header / content / player bar). UI callbacks reach the typed-
//! command layer; M2 wires that layer to `AppRuntime` and real auth.

slint::include_modules!();

mod commands;

use commands::AppCommand;

/// Central command sink. M2 replaces the body with real `AppRuntime`
/// dispatch; today it records that the typed-command path works.
fn dispatch(command: AppCommand) {
    log::info!("[qbz-slint] AppCommand::{} dispatched", command.id());
}

fn main() -> Result<(), slint::PlatformError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let window = AppWindow::new()?;

    // Sign-in and offline both reveal the app shell. During M1 this happens
    // without real authentication; M2 gates the transition on a real session.
    let weak = window.as_weak();
    window.on_sign_in_via_browser(move || {
        dispatch(AppCommand::SignInViaBrowser);
        if let Some(win) = weak.upgrade() {
            win.set_screen(AppScreen::Shell);
        }
    });

    window.on_use_system_browser(|| dispatch(AppCommand::UseSystemBrowser));

    let weak = window.as_weak();
    window.on_start_offline(move || {
        dispatch(AppCommand::StartOffline);
        if let Some(win) = weak.upgrade() {
            win.set_screen(AppScreen::Shell);
        }
    });

    window.on_open_tos(|| dispatch(AppCommand::OpenTermsOfService));

    log::info!("[qbz-slint] window ready — login screen");
    window.run()
}
