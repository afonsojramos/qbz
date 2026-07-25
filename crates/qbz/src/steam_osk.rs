//! Steam on-screen keyboard via the deeplink — Steam Deck plan PR 4
//! (qbz-nix-docs/steam-deck/STEAM_DECK_GAME_MODE_PLAN.md §5).
//!
//! The Deck OSK only opens when something asks the Steam client for it. For
//! a non-Steam shortcut the only programmatic path is the deeplink
//! `steam://open/keyboard?...&Mode=N` (and `steam://close/keyboard`) — the
//! same call Valve's own SDL fires on the Deck, proven for non-Steam apps.
//! Entered text arrives as ordinary key events, so no extra plumbing: a text
//! field gains focus → we fire open; it loses focus → we fire close. Mode 0
//! (single-line) covers every field in the app today (login is browser-OAuth,
//! so there is no in-app email field to flag Mode 2 for).
//!
//! Activation gate: `SteamDeck=1` env (set by SteamOS Game Mode) or
//! `SDL_ENABLE_STEAM_SCREEN_KEYBOARD=1`, with a manual override
//! `QBZ_STEAM_OSK=0/1` (0 wins over everything — diagnostics).

/// True when the Steam OSK integration should fire.
fn enabled() -> bool {
    match std::env::var("QBZ_STEAM_OSK") {
        Ok(v) if v == "1" => return true,
        Ok(v) if v == "0" => return false,
        _ => {}
    }
    std::env::var("SteamDeck").map(|v| v == "1").unwrap_or(false)
        || std::env::var_os("SDL_ENABLE_STEAM_SCREEN_KEYBOARD").is_some()
}

/// A text field's focus flipped (wired to `UiFocusState.osk` in main.rs).
/// Fire-and-forget: the deeplink has no dismissal callback, so ALWAYS send
/// close on blur — a stranded OSK is worse than a redundant close.
pub fn focus_changed(focused: bool, mode: i32) {
    if !enabled() {
        return;
    }
    let url = if focused {
        format!(
            "steam://open/keyboard?XPosition=0&YPosition=0&Width=0&Height=0&Mode={mode}"
        )
    } else {
        "steam://close/keyboard".to_string()
    };
    // xdg-open routes the steam:// scheme to the running Steam client; the
    // fallback pokes Steam directly (only if already running).
    if std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .is_err()
    {
        let _ = std::process::Command::new("steam")
            .arg("-ifrunning")
            .arg(&url)
            .spawn();
    }
    log::info!("[steam-osk] {} (mode {mode})", if focused { "open" } else { "close" });
}
