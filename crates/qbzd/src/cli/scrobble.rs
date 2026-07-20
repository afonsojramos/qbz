// crates/qbzd/src/cli/scrobble.rs — the `qbzd scrobble …` verbs (CONSOLE ext).
// Connect Last.fm / ListenBrainz and manage scrobbling, using the SAME
// methodology as `qbzd login`: Last.fm prints an authorize URL and exchanges
// the token after the user approves; ListenBrainz takes a pasted user token
// (like `login --token`). Credentials land in the daemon-root scrobbler store
// (crate::scrobbler); a running daemon is nudged to reload.
//
// These are LOCAL, daemon-down-capable operations (they write the store) — the
// same shape as `login`/`settings set`.
use std::io::{BufRead, Write};

use crate::cli::client::ApiClient;
use crate::paths::ProfileRoots;
use crate::scrobbler::{LastFmCreds, LbCreds, ScrobblerConfig};

/// `qbzd scrobble login lastfm` — the Last.fm web-auth flow (print URL →
/// user approves → exchange for a session key), mirroring `qbzd login`.
pub async fn login_lastfm(host: Option<String>, roots: &ProfileRoots) -> i32 {
    let mut client = qbz_integrations::lastfm::LastFmClient::new();
    let (token, auth_url) = match client.get_token().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: Last.fm token request failed: {e}");
            eprintln!("  → check your connection and retry");
            return 1;
        }
    };
    println!("Authorize QBZ on Last.fm, then come back here:");
    println!("  {auth_url}");
    print!("Press Enter after you've clicked \"Yes, allow access\"… ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);

    let session = match client.get_session(&token).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: Last.fm authorization not completed: {e}");
            eprintln!("  → approve access on the page first, then run this again");
            return 1;
        }
    };
    let mut cfg = ScrobblerConfig::load_at(&roots.config);
    cfg.lastfm = Some(LastFmCreds { session_key: session.key, username: session.name.clone(), enabled: true });
    if let Err(e) = cfg.save_at(&roots.config) {
        eprintln!("error: {e}");
        return 1;
    }
    nudge_reload(host).await;
    println!("Last.fm connected as {} — scrobbling enabled", session.name);
    0
}

/// `qbzd scrobble login listenbrainz --token <TOKEN>` — validate and store a
/// ListenBrainz user token (from listenbrainz.org/settings).
pub async fn login_listenbrainz(host: Option<String>, token: String, roots: &ProfileRoots) -> i32 {
    let client = qbz_integrations::listenbrainz::ListenBrainzClient::new();
    let info = match client.set_token(&token).await {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: ListenBrainz token rejected: {e}");
            eprintln!("  → get a token at https://listenbrainz.org/settings/");
            return 1;
        }
    };
    let mut cfg = ScrobblerConfig::load_at(&roots.config);
    cfg.listenbrainz = Some(LbCreds { token, username: info.user_name.clone(), enabled: true });
    if let Err(e) = cfg.save_at(&roots.config) {
        eprintln!("error: {e}");
        return 1;
    }
    nudge_reload(host).await;
    println!("ListenBrainz connected as {} — scrobbling enabled", info.user_name);
    0
}

/// `qbzd scrobble status` — per-provider connection + enabled state.
pub fn status(roots: &ProfileRoots) -> i32 {
    let cfg = ScrobblerConfig::load_at(&roots.config);
    println!("last.fm       : {}", provider_line(cfg.lastfm.as_ref().map(|l| (l.enabled, l.username.as_str()))));
    println!("listenbrainz  : {}", provider_line(cfg.listenbrainz.as_ref().map(|l| (l.enabled, l.username.as_str()))));
    0
}

/// `qbzd scrobble disable <lastfm|listenbrainz>` — keep the credentials but
/// stop scrobbling. `enable` re-enables.
pub async fn set_enabled(host: Option<String>, provider: String, enabled: bool, roots: &ProfileRoots) -> i32 {
    let mut cfg = ScrobblerConfig::load_at(&roots.config);
    let ok = match provider.as_str() {
        "lastfm" => cfg.lastfm.as_mut().map(|l| l.enabled = enabled).is_some(),
        "listenbrainz" => cfg.listenbrainz.as_mut().map(|l| l.enabled = enabled).is_some(),
        other => {
            eprintln!("error: unknown provider '{other}'");
            eprintln!("  → lastfm | listenbrainz");
            return 2;
        }
    };
    if !ok {
        eprintln!("error: {provider} is not connected");
        eprintln!("  → connect it first: qbzd scrobble login {provider}");
        return 1;
    }
    if let Err(e) = cfg.save_at(&roots.config) {
        eprintln!("error: {e}");
        return 1;
    }
    nudge_reload(host).await;
    println!("{provider} scrobbling {}", if enabled { "enabled" } else { "disabled" });
    0
}

// ============================ internals ============================

fn provider_line(v: Option<(bool, &str)>) -> String {
    match v {
        Some((true, name)) => format!("on as {name}"),
        Some((false, name)) => format!("off (connected as {name})"),
        None => "not connected".to_string(),
    }
}

/// Best-effort: tell a running daemon to reload (so scrobbler creds take effect
/// live). Silent if the daemon is down — the store write is what matters.
async fn nudge_reload(host: Option<String>) {
    let roots = crate::paths::ProfileRoots::resolve(None, None);
    let client = ApiClient::new(host, &roots);
    let _ = client.post("/api/settings/reload", serde_json::Value::Null).await;
}
