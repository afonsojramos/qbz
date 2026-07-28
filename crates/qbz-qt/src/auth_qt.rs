//! System-browser OAuth login for the Qt POC — a port of
//! `crates/qbz/src/auth.rs` onto cxx-qt, PLUS the redirect-listener
//! hardening from `crates/qbzd/src/login.rs` (CSPRNG nonce bound into the
//! redirect PATH; a mismatched/absent path nonce is dropped) that the
//! desktop auth.rs lacks.
//!
//! POC-NOTE (skipped vs the Slint glue, for the effort-measurement report):
//! the per-user store fan-out after a successful login/restore is NOT
//! ported. Omitted: `crate::offline::activate`, `offline_cache::
//! load_cached_ids`, `fav_cache`, `reco_dismiss`, `reco` (+train_async),
//! `external_reco`, `qbz_reco::ArtistVectorStore`, `discover_prefs`,
//! `artist_blacklist`, `pinned`, `local_favorites`, `search_service`,
//! `session_persist`, `lyrics`. Reason: the POC shell has no views that
//! read those stores; each is a mechanical `init_for_user(&dir)` call to
//! re-add when its view lands. Also omitted: the subscription purge
//! consumer (`spawn_subscription_purge_check`) — it needs the offline
//! cache, which this phase does not bring up; TODO when the offline cache
//! is wired.
//!
//! KEPT (the offline guardrails this phase is about):
//! `ensure_api_initialized` (scoped gate lift), `login_with_oauth_code`,
//! `core.set_session`, `runtime.activate`, offline-mode `init_for_user` +
//! `SubscriptionStateStore` bind + mark_valid/invalid (D4),
//! `engine().set_offline_session(false)` on success,
//! `qbz_credentials::save_oauth_token`, `qbz_log::register_secret`, and the
//! restore semantics (explicit auth rejection clears the token,
//! network-class failure keeps it).

use std::sync::Arc;
use std::time::Duration;

use qbz_app::shell::AppRuntime;
use qbz_app::user_data::UserDataPaths;
use qbz_core::FrontendAdapter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const OAUTH_TIMEOUT: Duration = Duration::from_secs(180);

/// The authenticated user, as the shell needs it.
pub struct SessionInfo {
    // Carried for future shell phases (per-user views); the phase-1 shell
    // placeholder only renders name + subscription.
    #[allow(dead_code)]
    pub user_id: u64,
    pub display_name: String,
    pub subscription: String,
}

/// Progress milestones of the browser OAuth, reported to the login UI so
/// the screen can narrate the flow.
#[derive(Clone, Copy, Debug)]
pub enum LoginPhase {
    /// The browser was opened; waiting for the redirect on the listener.
    WaitingForBrowser,
    /// The authorization code was captured; exchanging it for a session.
    Authenticating,
}

/// Run the full system-browser OAuth login. Returns the authenticated
/// session info on success. `on_phase` fires at each milestone; it is called
/// from this async context, so it must hop to the UI thread itself (the
/// caller wraps it in a `CxxQtThread::queue`).
pub async fn login_via_system_browser<A, F>(
    runtime: &Arc<AppRuntime<A>>,
    on_phase: F,
) -> Result<SessionInfo, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
    F: Fn(LoginPhase) + Send + Sync,
{
    let core = runtime.core();

    // No offline_session pre-clear (see the desktop note): the sign-in
    // endpoints are gate-exempt and the flag drops on SUCCESS only. The one
    // still-gated dependency — a cold bundle-token init — is scoped inside
    // ensure_api_initialized.
    ensure_api_initialized(core).await?;

    let app_id = {
        let client_lock = core.client();
        let guard = client_lock.read().await;
        let client = guard.as_ref().ok_or("Qobuz client not initialized")?;
        client.app_id().await.map_err(|e| e.to_string())?
    };

    // One-shot local listener for the OAuth redirect (loopback, ephemeral
    // port) — desktop behavior, unchanged.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind OAuth listener: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    // qbzd hardening: a CSPRNG nonce rides the redirect PATH and the
    // callback's request path must match it.
    let nonce = gen_nonce();
    let oauth_url = format!(
        "https://www.qobuz.com/signin/oauth?ext_app_id={}&redirect_url={}",
        app_id,
        urlencoding::encode(&format!("http://localhost:{port}/{nonce}")),
    );

    log::info!("[qbz-qt] opening system browser for OAuth (port {port})");
    open::that(&oauth_url).map_err(|e| format!("Failed to open browser: {e}"))?;
    on_phase(LoginPhase::WaitingForBrowser);

    let code = tokio::time::timeout(OAUTH_TIMEOUT, capture_oauth_code(listener, &nonce))
        .await
        .map_err(|_| "OAuth login timed out".to_string())?
        .ok_or_else(|| "OAuth login cancelled or no code received".to_string())?;

    log::info!("[qbz-qt] OAuth code captured, exchanging for session");
    on_phase(LoginPhase::Authenticating);

    let session = {
        let client_lock = core.client();
        let guard = client_lock.read().await;
        let client = guard.as_ref().ok_or("Qobuz client not initialized")?;
        match client.login_with_oauth_code(&code).await {
            Ok(session) => session,
            Err(e) => {
                // D4 producer: ONLY an explicit ineligible-account verdict
                // starts the grace clock. Generic 401/network errors never do.
                if matches!(e, qbz_qobuz::ApiError::IneligibleUser) {
                    crate::offline_fwd::subscription_mark_invalid();
                }
                return Err(e.to_string());
            }
        }
    };
    let user_id = session.user_id;
    let display_name = session.display_name.clone();
    let subscription = session.subscription_label.clone();
    let token = session.user_auth_token.clone();
    // Redaction (defense in depth): register the live auth token so any log
    // line that embeds it bare is scrubbed.
    qbz_log::register_secret(token.clone());

    // Emit LoggedIn through the core (idempotent set_session).
    core.set_session(session).await.map_err(|e| e.to_string())?;

    // Activate the per-user session (creates dirs, opens the session store).
    runtime.activate(user_id).await?;

    // POC-NOTE: per-user store fan-out skipped here (see module docs).

    // Offline-MODE per-user binding, then the D4 valid verdict and the D2
    // recovery: a successful login ends any unauthenticated offline session.
    if let Some(dir) = crate::offline_fwd::user_data_dir(user_id) {
        crate::offline_fwd::init_for_user(&dir);
        // Phase 5: local-favorites store (Library show-local + hearts).
        crate::library_qt::init_local_favorites(&dir);
        // Phase 7: pinned-items store (AlbumCard pin badges).
        crate::sidebar_qt::init_pinned(&dir);
    }
    crate::offline_fwd::subscription_mark_valid();
    crate::offline_fwd::engine().set_offline_session(false);

    // Persist the token so the next launch restores the session silently.
    if let Err(e) = qbz_credentials::save_oauth_token(&token) {
        log::warn!("[qbz-qt] failed to persist OAuth token: {e}");
    }

    log::info!("[qbz-qt] login complete for user {user_id}");
    Ok(SessionInfo {
        user_id,
        display_name,
        subscription,
    })
}

/// Make sure the Qobuz client holds bundle tokens before a sign-in call.
/// Scoped gate lift around the gated cold bundle init — verbatim port of
/// the desktop pattern (crates/qbz/src/auth.rs `ensure_api_initialized`).
async fn ensure_api_initialized<A>(core: &qbz_core::QbzCore<A>) -> Result<(), String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    if core.is_api_initialized().await {
        return Ok(());
    }
    let offline_engine = crate::offline_fwd::engine();
    let lifted = offline_engine.status().offline_session;
    if lifted {
        log::info!(
            "[qbz-qt] cold bundle init with an offline session active — lifting the flag for the init only"
        );
        offline_engine.set_offline_session(false);
    }
    let result = core.try_init_api().await.map_err(|e| e.to_string());
    if lifted {
        offline_engine.set_offline_session(true);
    }
    result
}

/// True only for an EXPLICIT auth rejection from Qobuz: a 401 on the token
/// login or an ineligible-account verdict. Network-class failures return
/// false — on those the saved token must be KEPT (spec §4.1 D1).
fn is_auth_rejection(error: &qbz_core::CoreError) -> bool {
    matches!(
        error,
        qbz_core::CoreError::Api(
            qbz_qobuz::ApiError::AuthenticationError(_) | qbz_qobuz::ApiError::IneligibleUser
        )
    )
}

/// Restore a previously saved session from the encrypted token store.
///
/// Returns `Ok(Some(SessionInfo))` when a saved token is valid and the
/// session is activated, `Ok(None)` when there is no token. A token that
/// exists but is explicitly rejected by Qobuz is cleared and treated as
/// `None`; on network-class failures the token is kept so the session can
/// still be restored later (offline boot, D2 recovery).
pub async fn restore_saved_session<A>(
    runtime: &Arc<AppRuntime<A>>,
) -> Result<Option<SessionInfo>, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let token = match qbz_credentials::load_oauth_token() {
        Ok(Some(token)) => token,
        Ok(None) => return Ok(None),
        Err(e) => {
            log::warn!("[qbz-qt] could not read saved token: {e}");
            return Ok(None);
        }
    };
    // Same redaction registration as the browser login path.
    qbz_log::register_secret(token.clone());

    let core = runtime.core();
    ensure_api_initialized(core).await?;

    match core.login_with_token(&token).await {
        Ok(session) => {
            let user_id = session.user_id;
            let display_name = session.display_name.clone();
            let subscription = session.subscription_label.clone();
            core.set_session(session).await.map_err(|e| e.to_string())?;
            runtime.activate(user_id).await?;
            // Per-user stores the shell depends on (pinned items for the
            // Home rail + pin badges, local favorites for hearts) — the
            // same binding the fresh-login path does (auth_qt login).
            // POC-NOTE: the rest of the Slint per-user store fan-out stays
            // skipped (see module docs).
            if let Some(dir) = crate::offline_fwd::user_data_dir(user_id) {
                crate::offline_fwd::init_for_user(&dir);
                crate::library_qt::init_local_favorites(&dir);
                crate::sidebar_qt::init_pinned(&dir);
            }
            crate::offline_fwd::subscription_mark_valid();
            crate::offline_fwd::engine().set_offline_session(false);
            log::info!("[qbz-qt] restored saved session for user {user_id}");
            Ok(Some(SessionInfo {
                user_id,
                display_name,
                subscription,
            }))
        }
        Err(e) if is_auth_rejection(&e) => {
            log::warn!("[qbz-qt] saved token rejected by Qobuz, clearing: {e}");
            let _ = qbz_credentials::clear_oauth_token();
            // D4 producer: only the explicit ineligible verdict starts the
            // grace clock; a plain 401 does not.
            if matches!(
                &e,
                qbz_core::CoreError::Api(qbz_qobuz::ApiError::IneligibleUser)
            ) {
                crate::offline_fwd::subscription_mark_invalid();
            }
            Ok(None)
        }
        Err(e) => {
            // Network-class failure (offline boot, timeout, 5xx, ...): KEEP
            // the token. The login screen shows with the session intact so
            // "Start offline" / the D2 recovery banner can use it later.
            log::warn!("[qbz-qt] session restore failed, keeping saved token: {e}");
            Ok(None)
        }
    }
}

/// "Start offline": activate an unauthenticated offline session at the last
/// known user (guest profile user 0 when none — `activate_offline` resolves
/// the same id internally), bind the offline-mode engine, and flag the
/// session-scoped offline state (D1). Returns the resolved user id.
///
/// POC-NOTE: the Slint `enter_shell_offline` also brings up the offline
/// cache, local library, per-user pref stores, tray and media controls —
/// all skipped here; the POC shell placeholder shows only the offline-mode
/// status. Re-add each `init_for_user(&dir)` as its view lands.
pub async fn start_offline_session<A>(runtime: &Arc<AppRuntime<A>>) -> Result<u64, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let user_id = UserDataPaths::load_last_user_id().unwrap_or(0);
    if user_id == 0 {
        log::info!(
            "[qbz-qt] offline shell: no previous session — entering the guest profile (user 0)"
        );
    }

    // Session scaffolding at the last user (session store, runtime state).
    runtime.activate_offline().await?;

    // Offline-MODE engine: bind the per-user stores and flag the
    // unauthenticated offline session.
    if let Some(dir) = crate::offline_fwd::user_data_dir(user_id) {
        crate::offline_fwd::init_for_user(&dir);
        // Phase 5: local-favorites store (Library show-local + hearts).
        crate::library_qt::init_local_favorites(&dir);
        // Phase 7: pinned-items store (AlbumCard pin badges).
        crate::sidebar_qt::init_pinned(&dir);
    }
    crate::offline_fwd::engine().set_offline_session(true);
    Ok(user_id)
}

/// Log out: clear the saved token, deactivate the per-user session, drop
/// the Qobuz client session, and tear down the offline-mode per-user state
/// (which reopens the Qobuz gate so a logged-out user can sign back in).
///
/// POC-NOTE: the Slint logout also tears down the skipped per-user stores
/// (offline cache, fav_cache, reco, artist vectors, discover prefs,
/// blacklist, pinned, local favorites, search, lyrics) — nothing to tear
/// down here since this phase never opens them.
pub async fn logout<A>(runtime: &Arc<AppRuntime<A>>) -> Result<(), String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let _ = qbz_credentials::clear_oauth_token();
    let _ = runtime.core().logout().await;
    crate::offline_fwd::teardown();
    runtime.deactivate().await?;
    log::info!("[qbz-qt] logged out");
    Ok(())
}

/// Accept connections until one carries a NONCE-VALID OAuth code, replying
/// with a minimal success page. Browser noise (favicon requests) and
/// nonce-mismatched requests are answered and skipped (qbzd hardening).
async fn capture_oauth_code(listener: TcpListener, expected_nonce: &str) -> Option<String> {
    loop {
        let (mut stream, _) = listener.accept().await.ok()?;
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).await.ok()?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let request_line = request.lines().next().unwrap_or("");
        let code = parse_callback(request_line, expected_nonce);

        let body = if code.is_some() {
            "<html><body style=\"font-family:system-ui;text-align:center;padding:64px;background:#0f0f0f;color:#fff\">\
             <h2>Login successful</h2><p>You can close this tab and return to QBZ.</p></body></html>"
        } else {
            "<html><body style=\"font-family:system-ui;text-align:center;padding:64px;background:#0f0f0f;color:#fff\">\
             <h2>Waiting for Qobuz...</h2></body></html>"
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;

        if code.is_some() {
            return code;
        }
    }
}

/// Returns the authorization code ONLY when the request PATH carries the
/// expected nonce (`GET /<nonce>?...`) — port of qbzd's `parse_callback`.
/// `code_autorisation` wins over `code`, matching the desktop.
fn parse_callback(request_line: &str, expected_nonce: &str) -> Option<String> {
    let target = request_line.split_whitespace().nth(1)?;
    let (path, query) = target.split_once('?')?;
    if path.trim_matches('/') != expected_nonce {
        return None; // wrong or absent path nonce → drop
    }
    query_param(query, "code_autorisation").or_else(|| query_param(query, "code"))
}

/// Extract and percent-decode a query parameter from a `&`-joined query
/// string (the part after `?`).
fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == key {
            return urlencoding::decode(v).ok().map(|s| s.into_owned());
        }
    }
    None
}

/// A 48-hex-char (24-byte) CSPRNG nonce, bound into the redirect-URL path
/// (port of qbzd's `gen_nonce`).
fn gen_nonce() -> String {
    use rand::RngExt;
    let mut bytes = [0u8; 24];
    rand::rng().fill(&mut bytes);
    let mut s = String::with_capacity(48);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_extracts_and_decodes() {
        assert_eq!(
            query_param("code_autorisation=abc123", "code_autorisation"),
            Some("abc123".to_string())
        );
        assert_eq!(query_param("a=1&code=x%2Fy", "code"), Some("x/y".to_string()));
        assert_eq!(query_param("other=1", "code"), None);
    }

    #[test]
    fn parse_callback_accepts_matching_path_nonce() {
        let line = "GET /abc123?code_autorisation=THECODE HTTP/1.1";
        assert_eq!(parse_callback(line, "abc123"), Some("THECODE".to_string()));
    }

    #[test]
    fn parse_callback_prefers_code_autorisation_over_code() {
        let line = "GET /n?code=plain&code_autorisation=preferred HTTP/1.1";
        assert_eq!(parse_callback(line, "n"), Some("preferred".to_string()));
    }

    #[test]
    fn parse_callback_rejects_mismatched_or_absent_path_nonce() {
        assert_eq!(
            parse_callback("GET /WRONG?code_autorisation=THECODE HTTP/1.1", "abc123"),
            None
        );
        assert_eq!(
            parse_callback("GET /?code_autorisation=THECODE HTTP/1.1", "abc123"),
            None
        );
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1", "abc123"), None);
    }

    #[test]
    fn gen_nonce_is_long_unique_and_hex() {
        let a = gen_nonce();
        let b = gen_nonce();
        assert_ne!(a, b);
        assert_eq!(a.len(), 48);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
