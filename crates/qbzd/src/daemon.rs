// crates/qbzd/src/daemon.rs — the `qbzd run` boot sequence (01-architecture.md
// §8.1, NORMATIVE order), the NeedsAuth-stays-up state machine (§6.2) and the
// graceful shutdown (§8.2). Later tasks splice into the numbered steps: the
// playback driver (T4) at step 10, the HTTP server (T6) at step 11, QConnect
// (T9/T10) at step 12. Until they land the daemon boots a playable core and
// parks on signals — API-less but fully diagnosable in-process.
use std::sync::{Arc, Mutex};

use qbz_app::shell::AppRuntime;
use qbz_core::CoreError;
use qbz_models::{CoreEvent, UserSession};
use tokio::sync::broadcast;

use crate::adapter::DaemonAdapter;
use crate::config::QbzdConfig;
use crate::lock::{InstanceLock, LockError};
use crate::paths::ProfileRoots;
use crate::state::{AuthState, DaemonShared, LatchedErrors, QconnectStatus};

/// The composed runtime handoff produced by [`boot`] and consumed by later
/// tasks: T4 spawns the playback driver on `runtime` + `shared`, T6 serves
/// `bus` over HTTP/SSE, T9/T10 wire QConnect. Held alive by [`run`] through the
/// signal park so the core stays up.
#[allow(dead_code)] // fields are the seam later tasks (T4/T6/T9/T10) read.
pub struct BootedRuntime {
    pub runtime: Arc<AppRuntime<DaemonAdapter>>,
    pub shared: Arc<Mutex<DaemonShared>>,
    pub bus: broadcast::Sender<CoreEvent>,
}

/// `qbzd run` — boot the daemon in the foreground, park on signals, shut down
/// gracefully. Returns the process exit code (0 = clean shutdown). `warns` are
/// the unknown-key warnings surfaced by [`QbzdConfig::load`] in `main`.
pub async fn run(roots: ProfileRoots, cfg: QbzdConfig, warns: Vec<String>) -> Result<i32, String> {
    // 1. argv parse happened in main(). 2. logging:
    qbz_log::install(&cfg.log.level);
    // 3. config: surface unknown-key warnings (they never abort — D14).
    for w in &warns {
        log::warn!("[config] unknown key: {w}");
    }
    // 4. instance lock on the DATA ROOT, taken BEFORE any port bind (§8.3): it,
    //    not the port, protects the single-device_uuid / single-session.db
    //    invariants. A second daemon on the same root is diagnosed → exit 3.
    let _lock = InstanceLock::acquire(&roots.data).map_err(diagnose_lock)?;
    // 5. port bind + foreign-occupant diagnosis (probe with GET /api/ping) is a
    //    stateless step that lands in T6 (api::bind); serving starts at step 11.
    //    Until T6 the daemon runs API-less.

    // 6.-9. compose stores + runtime + restore credentials.
    let booted = boot(&roots, &cfg, warns.len()).await?;

    // 10. playback driver (T4) · 11. HTTP serve (T6) · 12. QConnect (T9/T10)
    //     all splice here, reading `booted`.

    // 13. park on SIGTERM/SIGINT. NO startup audio "hygiene": both candidate
    //     fns are verified no-ops from a fresh process and re-adding them is the
    //     documented skeptic-correction #1 trap (§8.1).
    wait_for_signal().await;

    // ── Shutdown (§8.2, ordered). HTTP intake stop / QConnect disconnect /
    //    playback stop / final save_session+save_position arrive with their
    //    producing tasks (T4/T6/T9). Release the audio device by dropping the
    //    runtime (its Player) BEFORE the #521 pair (§8.2 step 3 precedes step 4).
    drop(booted);
    //    THE #521 PAIR runs unconditionally on Linux — exactly the desktop quit
    //    choke-point (crates/qbz/src/main.rs:20393): a forced PipeWire clock left
    //    set would pin the whole system's sample rate after the process dies.
    //    Both calls self-gate to no-ops when QBZ forced nothing.
    #[cfg(target_os = "linux")]
    {
        qbz_audio::alsa_backend::resume_suspended_sink();
        qbz_audio::pipewire_backend::PipeWireBackend::reset_pipewire_clock();
    }

    Ok(0) // instance lock released on drop of `_lock`
}

/// Steps 6-9 of §8.1: open the daemon-root stores, compose the runtime with the
/// two NORMATIVE substitutions (`with_audio_settings` + `activate_at`), and
/// restore the saved session per the §6.2 clearing taxonomy.
async fn boot(roots: &ProfileRoots, cfg: &QbzdConfig, warn_count: usize) -> Result<BootedRuntime, String> {
    // 6.+7. stores + runtime composition. The two substitutions (01 §2.2):
    //   - with_audio_settings, NOT AppRuntime::new (which hardcodes the
    //     desktop-global AudioSettingsStore — shell.rs:87-101);
    //   - activate_at (below), NOT activate (which resolves desktop
    //     UserDataPaths — shell.rs:195-203).
    // Everything routes through the T2 daemon roots.
    let store = qbz_audio::settings::AudioSettingsStore::new_at(&roots.data)?; // settings.rs:263
    let settings = store.get_settings()?;
    let (adapter, _rx) = DaemonAdapter::new();
    let bus = adapter.sender();
    let runtime = Arc::new(AppRuntime::with_audio_settings(
        adapter,
        settings.output_device.clone(),
        settings,
        None,
    )); // shell.rs:64

    // Offline-tolerant (§8.1-8): a network failure here still leaves a locally
    // usable core; a missing DAC is likewise non-fatal (Player starts deviceless
    // and retries with backoff — never the spotifyd #1097 crash-exit).
    if let Err(e) = runtime.init().await {
        log::warn!("core init did not complete (continuing offline-tolerant): {e}");
    }

    let shared = new_shared(cfg);
    if let Ok(mut s) = shared.lock() {
        s.startup_warnings = warn_count as u32;
    }

    // 8. credential restore per the §6.2 taxonomy (mirrors qbz/src/auth.rs:
    //    215-230): clear the token ONLY on explicit auth rejection; KEEP it on
    //    every network-class failure (clearing on transient errors is the
    //    documented boot-token-loss bug class).
    match qbz_credentials::load_oauth_token_at(&roots.config)? {
        None => set_needs_auth(&shared, None),
        Some(token) => {
            // Register before the token can reach any log line (§6.3).
            qbz_log::register_secret(token.clone());
            match runtime.core().login_with_token(&token).await {
                Ok(session) => {
                    restore_activate(&runtime, &shared, roots, session).await?;
                    // 9. session restore (queue/position) PAUSED — T4 wires
                    //    SessionStore::load_session here.
                }
                Err(e) if is_auth_rejection(&e) => {
                    qbz_credentials::clear_oauth_token_at(&roots.config)?;
                    latch_auth_error(&shared, &e);
                    set_needs_auth(&shared, Some(e));
                }
                Err(e) => {
                    // network-class: KEEP token, stay Restoring, retry w/ backoff.
                    log::warn!("session restore deferred (network-class): {e}");
                    spawn_auth_retry(runtime.clone(), shared.clone(), roots.clone());
                }
            }
        }
    }

    Ok(BootedRuntime {
        runtime,
        shared,
        bus,
    })
}

/// Activate the per-user session against DAEMON paths (§8.1-9): inject the
/// session into the core, then `activate_at` the runtime with per-user daemon
/// data/cache directories — never the desktop `UserDataPaths`.
async fn restore_activate(
    runtime: &Arc<AppRuntime<DaemonAdapter>>,
    shared: &Arc<Mutex<DaemonShared>>,
    roots: &ProfileRoots,
    session: UserSession,
) -> Result<(), String> {
    runtime
        .core()
        .set_session(session.clone())
        .await
        .map_err(|e| e.to_string())?;
    runtime
        .activate_at(
            session.user_id,
            &roots.data.join(format!("users/{}", session.user_id)),
            &roots.cache.join(format!("users/{}", session.user_id)),
        )
        .await?;
    set_logged_in(shared, &session);
    Ok(())
}

/// Fresh shared state. Starts in `Restoring` — credential restore drives the
/// terminal transition to `LoggedIn` or `NeedsAuth` (§6.2 diagram).
fn new_shared(cfg: &QbzdConfig) -> Arc<Mutex<DaemonShared>> {
    let _ = cfg; // reserved: premute/mpris defaults wire in with later tasks.
    Arc::new(Mutex::new(DaemonShared {
        auth: AuthState::Restoring,
        user_id: None,
        subscription: None,
        last_errors: LatchedErrors::default(),
        driver_last_tick: None,
        muted: false,
        premute_volume: 1.0,
        started_at: std::time::Instant::now(),
        startup_warnings: 0,
        qconnect: QconnectStatus::default(),
    }))
}

/// Enter NeedsAuth. `err = None` = no saved credentials at all (the common
/// first-run case); `Some(e)` = an explicit auth rejection just cleared the
/// token. Either way the daemon STAYS UP (§6.2) and names the fix.
fn set_needs_auth(shared: &Arc<Mutex<DaemonShared>>, err: Option<CoreError>) {
    if let Ok(mut s) = shared.lock() {
        s.auth = AuthState::NeedsAuth;
        s.user_id = None;
        s.subscription = None;
    }
    match err {
        None => log::info!("Not logged in — run 'qbzd setup' (or 'qbzd login')"),
        Some(e) => {
            log::warn!("Qobuz rejected the saved session ({e}) — run 'qbzd login' to re-authenticate")
        }
    }
}

/// Enter LoggedIn (Ready). Records the user id + subscription label for
/// `/api/status` (T6). The auth token itself is never stored here — it is a
/// registered secret and lives only in the credential file.
fn set_logged_in(shared: &Arc<Mutex<DaemonShared>>, session: &UserSession) {
    if let Ok(mut s) = shared.lock() {
        s.auth = AuthState::LoggedIn;
        s.user_id = Some(session.user_id);
        s.subscription = Some(session.subscription_label.clone());
    }
    log::info!(
        "Logged in (user {}, subscription '{}')",
        session.user_id,
        session.subscription_label
    );
}

/// Latch an auth error so a `status` call remains diagnosable after the fact
/// (§9.4 — drain-once channels alone cannot answer "why did the music stop?").
fn latch_auth_error(shared: &Arc<Mutex<DaemonShared>>, e: &CoreError) {
    if let Ok(mut s) = shared.lock() {
        s.last_errors.auth = Some(format!("token rejected by Qobuz — cleared ({e})"));
    }
}

/// True ONLY for an explicit auth rejection from Qobuz — a 401 on the token
/// login (`AuthenticationError`) or an ineligible-account verdict. Network
/// failures, offline gate, 5xx, rate limiting and parse errors all return false
/// so the saved token is KEPT (mirrors crates/qbz/src/auth.rs:215-230; the
/// taxonomy — not the variant list — is the normative part).
fn is_auth_rejection(error: &CoreError) -> bool {
    matches!(
        error,
        CoreError::Api(
            qbz_qobuz::ApiError::AuthenticationError(_) | qbz_qobuz::ApiError::IneligibleUser
        )
    )
}

/// Background retry for a network-class restore failure (§6.2: stay in the
/// authenticating state, KEEP the token, retry with backoff). On success the
/// session activates; on a now-explicit auth rejection the token is cleared and
/// the daemon drops to NeedsAuth; if the whole schedule sees only network-class
/// failures the token is KEPT and the daemon surfaces NeedsAuth so it stays
/// diagnosable and a later `qbzd login` / settings reload can retry.
fn spawn_auth_retry(
    runtime: Arc<AppRuntime<DaemonAdapter>>,
    shared: Arc<Mutex<DaemonShared>>,
    roots: ProfileRoots,
) {
    const SCHEDULE_SECS: [u64; 4] = [2, 5, 15, 30];
    tokio::spawn(async move {
        let token = match qbz_credentials::load_oauth_token_at(&roots.config) {
            Ok(Some(t)) => t,
            _ => return, // token vanished (concurrent logout) — nothing to retry.
        };
        for (i, delay) in SCHEDULE_SECS.iter().enumerate() {
            tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
            log::info!("session restore retry {}/{}", i + 1, SCHEDULE_SECS.len());
            match runtime.core().login_with_token(&token).await {
                Ok(session) => {
                    if let Err(e) = restore_activate(&runtime, &shared, &roots, session).await {
                        log::warn!("session activation after retry failed: {e}");
                    }
                    return;
                }
                Err(e) if is_auth_rejection(&e) => {
                    let _ = qbz_credentials::clear_oauth_token_at(&roots.config);
                    latch_auth_error(&shared, &e);
                    set_needs_auth(&shared, Some(e));
                    return;
                }
                Err(e) => log::warn!("session restore retry {} failed (network-class): {e}", i + 1),
            }
        }
        // Schedule exhausted with only network-class failures: KEEP the token,
        // surface NeedsAuth, latch the reason for `qbzd status`.
        if let Ok(mut s) = shared.lock() {
            s.auth = AuthState::NeedsAuth;
            s.last_errors.auth = Some(
                "could not reach Qobuz to restore the saved session — token kept, retry with 'qbzd login' or 'qbzd settings reload'".into(),
            );
        }
        log::warn!(
            "session restore gave up after {} network-class attempts — token KEPT",
            SCHEDULE_SECS.len()
        );
    });
}

/// Render an [`InstanceLock`] failure. For the already-running case this prints
/// the frozen exit-3 error voice (02 §1.3/§1.4) and exits 3 directly — the new
/// process must never clobber the running one. An I/O failure returns a String
/// that propagates to a generic exit 1.
fn diagnose_lock(e: LockError) -> String {
    match e {
        LockError::AlreadyRunning(pid) => {
            let who = pid
                .map(|p| format!("(pid {p})"))
                .unwrap_or_else(|| "(pid unknown)".to_string());
            eprintln!("error: qbzd is already running {who}");
            eprintln!("  → stop it first:  systemctl --user stop qbzd");
            eprintln!("  → or inspect it:  systemctl --user status qbzd");
            std::process::exit(3);
        }
        LockError::Io(msg) => {
            format!("error: could not take the instance lock: {msg}\n  → check permissions on the data root")
        }
    }
}

/// Park until SIGTERM or SIGINT. A second signal after this returns lets the
/// default handler take over → immediate exit (§8.2).
async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) {
            (Ok(mut term), Ok(mut int)) => {
                tokio::select! {
                    _ = term.recv() => log::info!("SIGTERM received — shutting down"),
                    _ = int.recv()  => log::info!("SIGINT received — shutting down"),
                }
            }
            _ => {
                // Fall back to Ctrl-C if the SIGTERM handler could not install.
                let _ = tokio::signal::ctrl_c().await;
                log::info!("Ctrl-C received — shutting down");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Ctrl-C received — shutting down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_auth_rejection_matches_only_explicit_rejections() {
        // Explicit rejections → clear the token.
        assert!(is_auth_rejection(&CoreError::Api(
            qbz_qobuz::ApiError::AuthenticationError("401".into())
        )));
        assert!(is_auth_rejection(&CoreError::Api(
            qbz_qobuz::ApiError::IneligibleUser
        )));
        // Network-class / other → KEEP the token (the boot-token-loss guard).
        assert!(!is_auth_rejection(&CoreError::Api(
            qbz_qobuz::ApiError::ServerError(503)
        )));
        assert!(!is_auth_rejection(&CoreError::Api(
            qbz_qobuz::ApiError::RateLimited(30)
        )));
        assert!(!is_auth_rejection(&CoreError::NotInitialized));
    }

    #[test]
    fn no_credentials_enters_needs_auth() {
        let shared = new_shared(&QbzdConfig::default());
        set_needs_auth(&shared, None);
        let s = shared.lock().unwrap();
        assert_eq!(s.auth, AuthState::NeedsAuth);
        assert!(s.user_id.is_none());
        assert!(s.last_errors.auth.is_none());
    }

    #[test]
    fn explicit_rejection_latches_and_needs_auth() {
        let shared = new_shared(&QbzdConfig::default());
        let err = CoreError::Api(qbz_qobuz::ApiError::AuthenticationError("401".into()));
        latch_auth_error(&shared, &err);
        set_needs_auth(&shared, Some(err));
        let s = shared.lock().unwrap();
        assert_eq!(s.auth, AuthState::NeedsAuth);
        assert!(s.last_errors.auth.is_some());
    }

    #[test]
    fn logged_in_records_user_and_subscription() {
        let shared = new_shared(&QbzdConfig::default());
        let session = UserSession {
            user_auth_token: "secret".into(),
            user_id: 1234567,
            email: "a@b.c".into(),
            display_name: "Tester".into(),
            subscription_label: "studio".into(),
            subscription_valid_until: None,
        };
        set_logged_in(&shared, &session);
        let s = shared.lock().unwrap();
        assert_eq!(s.auth, AuthState::LoggedIn);
        assert_eq!(s.user_id, Some(1234567));
        assert_eq!(s.subscription.as_deref(), Some("studio"));
    }
}
