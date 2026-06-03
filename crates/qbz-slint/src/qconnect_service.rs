//! Slint QConnect service (pieces c + d, Phase S).
//!
//! `SlintQconnectService` is the connect-flow facade for the Slint frontend: it
//! owns the connection lifecycle and reproduces the Tauri
//! `QconnectServiceState::connect` recipe (build transport -> one shared
//! sync-state Mutex -> sink -> `QconnectApp::new` -> `set_app` -> `connect` ->
//! subscribe transport events BEFORE the spawn -> spawn `run_session_loop`), plus
//! controller bootstrap (JoinSession + AskForQueueState) and the deferred
//! renderer-join.
//!
//! `SlintSessionLoopHost` implements the frontend-agnostic
//! `qconnect_app::SessionLoopHost` so the shared session loop drives lifecycle,
//! reconnect bootstrap/resync, deferred renderer-join, and reconnect-exhausted
//! teardown through this adapter — exactly as `TauriSessionLoopHost` does.

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qconnect_app::renderer::PLAYING_STATE_STOPPED;
use qconnect_app::{
    is_local_renderer_active, QconnectApp, QconnectAppEvent, QconnectEventSink,
    QconnectFileAudioQualitySnapshot, QconnectLifecycleState, QconnectRemoteSyncState,
    QueueCommandType, RendererReport, RendererReportType, SessionLoopHost,
};
use qconnect_transport_ws::{NativeWsTransport, WsTransportConfig};
use serde_json::json;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::adapter::SlintAdapter;
use crate::qconnect_engine::SlintRendererEngine;
use crate::qconnect_event_sink::{SlintQconnectApp, SlintQconnectEventSink};
use crate::qconnect_transport::{
    default_qconnect_device_info, default_qconnect_device_info_with_name, load_persisted_device_name,
    resolve_transport_config, QconnectJoinSessionRequest, AUDIO_QUALITY_HIRES_LEVEL2,
    BUFFER_STATE_OK,
};
use crate::AppWindow;

type Runtime = Arc<AppRuntime<SlintAdapter>>;

/// Process-wide QConnect service singleton (one per app, like the playback
/// QueueController). Initialized once at shell setup; the connect trigger + the
/// future `*_if_remote` transport routing reach it through `service()`.
static SERVICE: std::sync::OnceLock<Arc<SlintQconnectService>> = std::sync::OnceLock::new();

/// Initialize the QConnect service singleton (idempotent — a second call returns
/// the existing instance, ignoring the new args).
pub fn init_service(runtime: Runtime, window: slint::Weak<AppWindow>) -> Arc<SlintQconnectService> {
    SERVICE
        .get_or_init(|| Arc::new(SlintQconnectService::new(runtime, window)))
        .clone()
}

/// The initialized QConnect service, if shell setup has run.
pub fn service() -> Option<Arc<SlintQconnectService>> {
    SERVICE.get().cloned()
}

// ---- DEV diagnostics (QconnectDevModal) ------------------------------------
// A rolling, runtime-inspectable event log + live status block, so QConnect can
// be debugged WITHOUT a rebuild (Slint builds are slow). Populated by the event
// sink; rendered by `ui/shell/QconnectDevModal.slint`.

static DEV_LOG: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<String>>> =
    std::sync::OnceLock::new();
static DEV_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
const DEV_LOG_CAP: usize = 150;

fn dev_log_text(push: Option<String>, clear: bool) -> String {
    let buf = DEV_LOG.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    let mut guard = buf.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if clear {
        guard.clear();
    }
    if let Some(line) = push {
        guard.push_front(line);
        while guard.len() > DEV_LOG_CAP {
            guard.pop_back();
        }
    }
    guard.iter().cloned().collect::<Vec<_>>().join("\n")
}

/// Append a formatted event line (with a relative timestamp) to the DEV log and
/// push the joined text to the modal. Called for every inbound QConnect event.
pub fn dev_push_event(weak: &slint::Weak<AppWindow>, line: String) {
    let start = DEV_START.get_or_init(std::time::Instant::now);
    let ms = start.elapsed().as_millis();
    let text = dev_log_text(Some(format!("[{ms}ms] {line}")), false);
    let _ = weak.upgrade_in_event_loop(move |w| {
        use slint::ComponentHandle;
        w.global::<crate::QconnectDevState>().set_log_text(text.into());
    });
}

/// Replace the DEV status block (session topology / renderer roles / queue).
pub fn dev_set_status(weak: &slint::Weak<AppWindow>, status: String) {
    let _ = weak.upgrade_in_event_loop(move |w| {
        use slint::ComponentHandle;
        w.global::<crate::QconnectDevState>()
            .set_status(status.into());
    });
}

/// Clear the DEV event log (wired to `QconnectDevState.clear()`).
pub fn dev_clear(weak: &slint::Weak<AppWindow>) {
    let text = dev_log_text(None, true);
    let _ = weak.upgrade_in_event_loop(move |w| {
        use slint::ComponentHandle;
        w.global::<crate::QconnectDevState>().set_log_text(text.into());
    });
}

struct SlintQconnectRuntime {
    app: Arc<SlintQconnectApp>,
    #[allow(dead_code)] // retained for status/endpoint reporting in the UI step
    config: WsTransportConfig,
    event_loop: tokio::task::JoinHandle<()>,
    #[allow(dead_code)] // shared with the app + sink; kept for future snapshot queries
    sync_state: Arc<Mutex<QconnectRemoteSyncState>>,
}

#[derive(Default)]
struct SlintQconnectInner {
    runtime: Option<SlintQconnectRuntime>,
    last_error: Option<String>,
    lifecycle_state: QconnectLifecycleState,
}

/// Dedup + gate a lifecycle transition: only emit while a runtime is alive and
/// the state actually changes. Mirrors the Tauri `update_lifecycle_state_if_running`.
async fn update_lifecycle_state_if_running(
    inner: &Arc<Mutex<SlintQconnectInner>>,
    sink: &SlintQconnectEventSink,
    next: QconnectLifecycleState,
) {
    let mut guard = inner.lock().await;
    if guard.runtime.is_none() {
        return;
    }
    if guard.lifecycle_state == next {
        return;
    }
    guard.lifecycle_state = next;
    drop(guard);
    sink.on_event(QconnectAppEvent::LifecycleChanged { state: next })
        .await;
}

pub struct SlintQconnectService {
    inner: Arc<Mutex<SlintQconnectInner>>,
    runtime: Runtime,
    window: slint::Weak<AppWindow>,
    #[allow(dead_code)] // wired to the device-name settings UI in a later step
    custom_device_name: Arc<tokio::sync::RwLock<Option<String>>>,
    /// Track-id list of the queue we last pushed to the session (controller-side
    /// queue sync). Guards against re-pushing the same queue before the cloud's
    /// echo `QueueUpdated` lands (which would otherwise double-push on the next
    /// track tick). Cleared on disconnect.
    last_pushed_queue_ids: Mutex<Option<Vec<u64>>>,
}

impl SlintQconnectService {
    pub fn new(runtime: Runtime, window: slint::Weak<AppWindow>) -> Self {
        let saved_name = load_persisted_device_name();
        Self {
            inner: Arc::new(Mutex::new(SlintQconnectInner::default())),
            runtime,
            window,
            custom_device_name: Arc::new(tokio::sync::RwLock::new(saved_name)),
            last_pushed_queue_ids: Mutex::new(None),
        }
    }

    pub async fn is_running(&self) -> bool {
        self.inner.lock().await.runtime.is_some()
    }

    /// Report this device's playback state to the cloud while QBZ is the ACTIVE
    /// LOCAL renderer (driven by the playback poll loop). Mirrors the Tauri
    /// `v2_qconnect_report_playback_state` essentials: self-gates on
    /// is_local_renderer_active (no-op when not connected, or when a PEER owns
    /// playback), resolves the current/next queue_item_id from the playing track
    /// (the frontend doesn't track qids), sends a RndrSrvrStateUpdated, and keeps
    /// the app's renderer position in sync. `position_ms`/`duration_ms` are in
    /// MILLISECONDS (the QConnect protocol unit).
    pub async fn report_playback_state(
        &self,
        playing_state: i32,
        position_ms: i64,
        duration_ms: i64,
        track_id: u64,
    ) {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            match guard.runtime.as_ref() {
                Some(runtime) => (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state)),
                None => return,
            }
        };

        // Only report when WE are the active renderer. When a peer renderer owns
        // playback (QBZ is acting as a controller) the renderer reports come from
        // the peer, not us.
        {
            let state = sync_state.lock().await;
            if !is_local_renderer_active(&state.session) {
                return;
            }
        }

        let (current_qid, next_qid) =
            resolve_queue_item_ids_by_track_id(&app, &sync_state, track_id).await;
        let queue_version = app.queue_state_snapshot().await.version;

        let report = RendererReport::new(
            RendererReportType::RndrSrvrStateUpdated,
            Uuid::new_v4().to_string(),
            queue_version,
            json!({
                "playing_state": playing_state,
                "buffer_state": BUFFER_STATE_OK,
                "current_position": position_ms,
                "duration": duration_ms,
                "current_queue_item_id": current_qid,
                "next_queue_item_id": next_qid,
                "queue_version": {
                    "major": queue_version.major,
                    "minor": queue_version.minor
                }
            }),
        );
        if let Err(err) = app.send_renderer_report_command(report).await {
            log::warn!("[QConnect] Failed to report playback state: {err}");
        }

        if position_ms >= 0 {
            app.update_renderer_position(position_ms as u64).await;
        }

        // Report the live output format so the controller shows the correct
        // quality badge (CD / Hi-Res). Reads the player's current output
        // (sample_rate/bit_depth); channels default to stereo. Both reports dedup
        // internally in qconnect-app, so calling them every report tick is cheap.
        let player = self.runtime.core().player();
        let sample_rate = player.state.get_sample_rate();
        let bit_depth = player.state.get_bit_depth();
        if let Some(snapshot) =
            build_file_audio_quality_snapshot(sample_rate, bit_depth, QCONNECT_RENDERER_CHANNELS)
        {
            if let Err(err) = app
                .report_file_audio_quality_if_changed(queue_version, snapshot)
                .await
            {
                log::warn!("[QConnect] Failed to report file audio quality: {err}");
            }
            if let Err(err) = app
                .report_device_audio_quality_if_changed(
                    queue_version,
                    snapshot.sampling_rate,
                    snapshot.bit_depth,
                    snapshot.nb_channels,
                )
                .await
            {
                log::warn!("[QConnect] Failed to report device audio quality: {err}");
            }
        }
    }

    /// Establish the QConnect session. Gated on an initialized API client (the
    /// qws/createToken discovery needs it). Idempotent: a second call while a
    /// runtime is alive is a no-op (the UI toggle stays on).
    pub async fn connect(&self) -> Result<(), String> {
        if !self.runtime.core().is_api_initialized().await {
            return Err("Qobuz API is not initialized; cannot start Qobuz Connect".to_string());
        }

        // Claim the connect slot ATOMICALLY before the transport-config await, so
        // two concurrent connect()s can't both build a runtime (the second would
        // overwrite + leak the first's transport + event-loop task). A live
        // runtime OR an in-flight `Connecting` both short-circuit to a no-op —
        // the re-check the adversarial review flagged (`runtime.is_some()` only)
        // had a TOCTOU window across the await; the `Connecting` sentinel closes it.
        {
            let mut guard = self.inner.lock().await;
            if guard.runtime.is_some()
                || guard.lifecycle_state == QconnectLifecycleState::Connecting
            {
                log::info!(
                    "[QConnect] connect() called while already {:?}; no-op",
                    guard.lifecycle_state
                );
                return Ok(());
            }
            guard.lifecycle_state = QconnectLifecycleState::Connecting;
            guard.last_error = None;
        }

        let config = match resolve_transport_config(&self.runtime).await {
            Ok(config) => config,
            Err(err) => {
                // Release the Connecting claim so a later retry can proceed.
                let mut guard = self.inner.lock().await;
                if guard.runtime.is_none() {
                    guard.lifecycle_state = QconnectLifecycleState::Off;
                }
                return Err(err);
            }
        };

        let transport = Arc::new(NativeWsTransport::new());
        let sync_state = Arc::new(Mutex::new(QconnectRemoteSyncState::default()));
        let engine = SlintRendererEngine::new(Arc::clone(&self.runtime));
        let sink = Arc::new(SlintQconnectEventSink::new(
            engine,
            Arc::clone(&self.runtime),
            Arc::clone(&sync_state),
            self.window.clone(),
        ));
        let app = Arc::new(QconnectApp::new(
            Arc::clone(&transport) as Arc<NativeWsTransport>,
            Arc::clone(&sink),
            Arc::clone(&sync_state),
        ));
        // Wire the owning app into the sink so it can emit reports (e.g.
        // is_active=true after SetActive(true)) and drive session-apply.
        sink.set_app(&app);

        if let Err(err) = app.connect(config.clone()).await {
            let mut guard = self.inner.lock().await;
            guard.lifecycle_state = QconnectLifecycleState::Off;
            let msg = format!("qconnect transport connect failed: {err}");
            guard.last_error = Some(msg.clone());
            return Err(msg);
        }

        // Subscribe to transport events SYNCHRONOUSLY here — after connect()
        // returns and BEFORE the spawn / any further await — so the receiver is
        // live before the WS handshake emits Connected / Subscribed /
        // SessionEstablished / SESSION_STATE. tokio broadcast has no replay; a
        // receiver created inside the spawned loop would race + drop those.
        let transport_rx = app.subscribe_transport_events();
        let idle_retry_active = config.reconnect_idle_retry_ms > 0;
        let host: Arc<dyn SessionLoopHost> = Arc::new(SlintSessionLoopHost {
            app: Arc::clone(&app),
            sync_state: Arc::clone(&sync_state),
            inner: Arc::clone(&self.inner),
            sink: Arc::clone(&sink),
            runtime: Arc::clone(&self.runtime),
        });
        let app_for_loop = Arc::clone(&app);
        let event_loop = tokio::spawn(async move {
            app_for_loop
                .run_session_loop(host, transport_rx, idle_retry_active)
                .await;
        });

        let runtime_app = Arc::clone(&app);
        {
            let mut guard = self.inner.lock().await;
            guard.last_error = None;
            guard.runtime = Some(SlintQconnectRuntime {
                app,
                config,
                event_loop,
                sync_state,
            });
        }

        let custom_name = self.custom_device_name.read().await.clone();
        if let Err(err) = bootstrap_remote_presence(&runtime_app, custom_name).await {
            let _ = self.disconnect().await;
            let mut guard = self.inner.lock().await;
            guard.last_error = Some(format!("qconnect bootstrap failed: {err}"));
            return Err(format!("qconnect bootstrap failed: {err}"));
        }

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), String> {
        let runtime = {
            let mut guard = self.inner.lock().await;
            // Always force Off — "disable QConnect" must succeed regardless of the
            // current lifecycle (issue #358). The transport shutdown + the loop
            // abort tear down any in-flight reconnect.
            guard.lifecycle_state = QconnectLifecycleState::Off;
            guard.runtime.take()
        };

        *self.last_pushed_queue_ids.lock().await = None;

        if let Some(runtime) = runtime {
            if let Err(err) = runtime.app.disconnect().await {
                let mut guard = self.inner.lock().await;
                guard.last_error = Some(format!("qconnect disconnect failed: {err}"));
            }
            runtime.event_loop.abort();
        }

        Ok(())
    }

    /// Controller-side queue sync: when the LOCAL queue differs from the session
    /// queue (the user started a new album/playlist on QBZ while connected), push
    /// it to the session so the controller (e.g. the iOS app) sees it. Called from
    /// the playback poll loop on each track transition.
    ///
    /// Echo-safe by construction: the inbound materialize path never calls this,
    /// and we skip when the local queue already equals the cloud's last-applied
    /// queue OR the last queue we pushed. Admission mirrors the webplayer's
    /// `assessQconnectQueueSync` — all-or-nothing: a queue containing any local /
    /// Plex track is refused whole (a renderer can only play Qobuz catalog ids;
    /// offline qobuz_download IS eligible — its id is the real Qobuz id).
    pub async fn sync_local_queue_if_changed(&self) {
        let (app, sync_state) = {
            let guard = self.inner.lock().await;
            match guard.runtime.as_ref() {
                Some(runtime) => (Arc::clone(&runtime.app), Arc::clone(&runtime.sync_state)),
                None => return,
            }
        };

        // Only push while WE are the active renderer (the user is driving QBZ).
        {
            let state = sync_state.lock().await;
            if !is_local_renderer_active(&state.session) {
                return;
            }
        }

        let (tracks, current_index) = self.runtime.core().get_all_queue_tracks().await;
        if tracks.is_empty() {
            return;
        }
        let ordered_ids: Vec<u64> = tracks.iter().map(|track| track.id).collect();

        // Echo-suppress: skip when this is the cloud's current queue (materialized
        // inbound) so our own adoption / a remote queue change never bounces back.
        {
            let state = sync_state.lock().await;
            if let Some(applied) = &state.last_applied_queue_state {
                let applied_ids: Vec<u64> =
                    applied.queue_items.iter().map(|item| item.track_id).collect();
                if applied_ids == ordered_ids {
                    return;
                }
            }
        }
        // ...and skip when we already pushed this exact queue (cloud echo pending).
        {
            let pushed = self.last_pushed_queue_ids.lock().await;
            if pushed.as_deref() == Some(ordered_ids.as_slice()) {
                return;
            }
        }

        // Admission: refuse the whole push if any track isn't Qobuz-castable.
        let all_eligible = tracks.iter().all(|track| {
            let source = track
                .source
                .as_deref()
                .unwrap_or("qobuz")
                .to_ascii_lowercase();
            source != "local" && source != "plex" && track.id > 0
        });
        if !all_eligible {
            log::info!("[QConnect] Local queue has non-Qobuz tracks; not casting to Connect");
            crate::toast::error_weak(&self.window, "Mixed queue — not cast to Qobuz Connect");
            dev_push_event(&self.window, "-> queue push REFUSED (mixed/non-Qobuz)".to_string());
            // Remember it so we don't re-toast on every track tick within this queue.
            *self.last_pushed_queue_ids.lock().await = Some(ordered_ids);
            return;
        }

        let count = ordered_ids.len();
        let track_ids: Vec<i64> = ordered_ids.iter().map(|id| *id as i64).collect();
        let start_index = current_index.unwrap_or(0);
        let payload = json!({
            "track_ids": track_ids,
            "queue_position": start_index,
            "shuffle_mode": false,
            "shuffle_pivot_index": start_index,
            "context_uuid": Uuid::new_v4().to_string(),
            "autoplay_reset": true,
            "autoplay_loading": false,
        });
        let command = app
            .build_queue_command(QueueCommandType::CtrlSrvrQueueLoadTracks, payload)
            .await;
        match app.send_queue_command(command).await {
            Ok(_) => {
                log::info!(
                    "[QConnect] Pushed local queue to Connect ({count} tracks, start={start_index})"
                );
                dev_push_event(
                    &self.window,
                    format!("-> QueueLoadTracks {count} tracks start={start_index}"),
                );
                *self.last_pushed_queue_ids.lock().await = Some(ordered_ids);
            }
            Err(err) => log::warn!("[QConnect] Failed to push local queue: {err}"),
        }
    }
}

/// Slint-side implementation of the shared session-loop seams (piece c). Holds
/// the handles the loop reaches back into: the app (renderer join + state reads),
/// the shared sync accumulator, the service inner (lifecycle gating + teardown),
/// the sink (lifecycle emit), and the runtime (track duration read for the join).
struct SlintSessionLoopHost {
    app: Arc<SlintQconnectApp>,
    sync_state: Arc<Mutex<QconnectRemoteSyncState>>,
    inner: Arc<Mutex<SlintQconnectInner>>,
    sink: Arc<SlintQconnectEventSink>,
    runtime: Runtime,
}

#[async_trait::async_trait]
impl SessionLoopHost for SlintSessionLoopHost {
    async fn update_lifecycle(&self, state: QconnectLifecycleState) {
        update_lifecycle_state_if_running(&self.inner, &self.sink, state).await;
    }

    async fn bootstrap_after_reconnect(&self) {
        if let Err(err) = bootstrap_remote_presence(&self.app, None).await {
            log::error!("[QConnect] Re-bootstrap after reconnect failed: {err}");
        }
    }

    async fn deferred_renderer_join(&self, session_uuid: String, reason: i32) {
        deferred_renderer_join(
            &self.app,
            &self.sync_state,
            &self.runtime,
            &session_uuid,
            reason,
        )
        .await;
    }

    async fn on_reconnect_exhausted(
        &self,
        attempts: u32,
        last_reason: String,
        idle_retry_active: bool,
    ) -> bool {
        {
            let mut guard = self.inner.lock().await;
            guard.lifecycle_state = QconnectLifecycleState::Exhausted;
            guard.last_error = Some(format!(
                "Reconnect attempts exhausted ({attempts}): {last_reason}"
            ));
            if !idle_retry_active {
                // Legacy terminate path: drop the runtime so a fresh connect()
                // succeeds. Dropping it detaches this task's own JoinHandle (fine
                // — the loop breaks right after) (#358).
                guard.runtime = None;
            }
        }
        // TODO(slint-qconnect-ui): surface the Exhausted lifecycle on the badge.
        log::warn!("[QConnect] Reconnect exhausted ({attempts}): {last_reason}");
        !idle_retry_active
    }

    async fn on_loop_error(&self, message: String) {
        // TODO(slint-qconnect-ui): surface as a toast (Tauri emits qconnect:error).
        log::error!("[QConnect] session loop error: {message}");
    }
}

const QCONNECT_RENDERER_CHANNELS: i32 = 2;
const AUDIO_QUALITY_UNKNOWN: i32 = 0;
const AUDIO_QUALITY_MP3: i32 = 1;
const AUDIO_QUALITY_CD: i32 = 2;
const AUDIO_QUALITY_HIRES_L1: i32 = 3;
const AUDIO_QUALITY_HIRES_L2: i32 = 4;
const AUDIO_QUALITY_HIRES_L3: i32 = 5;

/// Classify a (sample_rate, bit_depth) output into the QConnect AudioQuality
/// level. Pure mirror of the Tauri `classify_qconnect_audio_quality`.
fn classify_audio_quality(sample_rate: u32, bit_depth: u32) -> i32 {
    if sample_rate == 0 || bit_depth == 0 {
        AUDIO_QUALITY_UNKNOWN
    } else if sample_rate >= 384_000 {
        AUDIO_QUALITY_HIRES_L3
    } else if sample_rate >= 192_000 {
        AUDIO_QUALITY_HIRES_L2
    } else if bit_depth > 16 || sample_rate > 48_000 {
        AUDIO_QUALITY_HIRES_L1
    } else if sample_rate >= 44_100 {
        AUDIO_QUALITY_CD
    } else {
        AUDIO_QUALITY_MP3
    }
}

/// Build a file-audio-quality snapshot from the live output format, or None when
/// the format isn't known yet. Pure mirror of the Tauri
/// `build_qconnect_file_audio_quality_snapshot`.
fn build_file_audio_quality_snapshot(
    sample_rate: u32,
    bit_depth: u32,
    nb_channels: i32,
) -> Option<QconnectFileAudioQualitySnapshot> {
    if sample_rate == 0 || bit_depth == 0 {
        return None;
    }
    Some(QconnectFileAudioQualitySnapshot {
        sampling_rate: sample_rate as i32,
        bit_depth: bit_depth as i32,
        nb_channels,
        audio_quality: classify_audio_quality(sample_rate, bit_depth),
    })
}

/// Resolve the current + next `queue_item_id` for a playing `track_id` from the
/// cloud queue snapshot, caching the result into the sync accumulator. Mirrors
/// the Tauri `resolve_queue_item_ids_by_track_id`. Used by the renderer report so
/// the controller can map our playback to its queue rows.
async fn resolve_queue_item_ids_by_track_id(
    app: &Arc<SlintQconnectApp>,
    sync_state: &Arc<Mutex<QconnectRemoteSyncState>>,
    track_id: u64,
) -> (Option<u64>, Option<u64>) {
    let queue = app.queue_state_snapshot().await;
    let (current_qid, next_qid, next_track_id) =
        qconnect_app::queue_resolution::resolve_queue_item_ids_from_queue_state(&queue, track_id);

    if let Some(current_qid) = current_qid {
        let mut state = sync_state.lock().await;
        state.last_renderer_queue_item_id = Some(current_qid);
        state.last_renderer_next_queue_item_id = next_qid;
        state.last_renderer_track_id = Some(track_id);
        state.last_renderer_next_track_id = next_track_id;
        (Some(current_qid), next_qid)
    } else {
        (None, None)
    }
}

/// Controller-side bootstrap: JoinSession (works without a session_uuid) then ask
/// for the current queue state. The renderer-side join is deferred until the
/// server sends SESSION_STATE with a session_uuid (handled in the session loop).
/// Mirrors the Tauri `bootstrap_remote_presence`.
async fn bootstrap_remote_presence(
    app: &Arc<SlintQconnectApp>,
    custom_device_name: Option<String>,
) -> Result<(), String> {
    let device_info = default_qconnect_device_info_with_name(custom_device_name.as_deref());

    let join_payload = serde_json::to_value(QconnectJoinSessionRequest {
        session_uuid: None,
        device_info: Some(device_info),
    })
    .map_err(|err| format!("serialize join_session bootstrap payload: {err}"))?;

    let join_command = app
        .build_queue_command(QueueCommandType::CtrlSrvrJoinSession, join_payload)
        .await;
    let join_action_uuid = app
        .send_queue_command(join_command)
        .await
        .map_err(|err| format!("send bootstrap ctrl_srvr_join_session failed: {err}"))?;
    // JoinSession responds with session/renderer controller events not part of
    // queue reducer correlation. Drop the pending slot so queue ops aren't blocked.
    app.clear_pending_if_matches(&join_action_uuid).await;

    let ask_queue_command = app
        .build_queue_command(QueueCommandType::CtrlSrvrAskForQueueState, json!({}))
        .await;
    let ask_action_uuid = app
        .send_queue_command(ask_queue_command)
        .await
        .map_err(|err| format!("send bootstrap ask_for_queue_state failed: {err}"))?;
    app.clear_pending_if_matches(&ask_action_uuid).await;

    log::info!(
        "[QConnect] Bootstrap complete: controller joined, queue state requested. Renderer join deferred until session_uuid received."
    );
    Ok(())
}

/// Deferred renderer join: called from the session loop when SESSION_STATE with a
/// session_uuid arrives. Idempotent per uuid (P1-8). Mirrors the Tauri
/// `deferred_renderer_join`, reading the current track duration via
/// `runtime.core().get_track` instead of the Tauri CoreBridge.
async fn deferred_renderer_join(
    app: &Arc<SlintQconnectApp>,
    sync_state: &Arc<Mutex<QconnectRemoteSyncState>>,
    runtime: &Runtime,
    session_uuid: &str,
    join_reason: i32,
) {
    let already_joined = {
        let st = sync_state.lock().await;
        st.last_joined_session_uuid.as_deref() == Some(session_uuid)
    };
    if already_joined {
        log::info!(
            "[QConnect] Deferred join skipped (already joined session_uuid={session_uuid}); re-asking renderer state"
        );
        if let Err(err) = app.ask_for_active_renderer_state().await {
            log::warn!("[QConnect] Idempotent-join AskForRendererState failed: {err}");
        }
        return;
    }

    let device_info = default_qconnect_device_info();
    let queue_version_ref = app.queue_state_snapshot().await.version;

    log::info!("[QConnect] Deferred renderer join with session_uuid={session_uuid}");

    // 1. Renderer JoinSession with session_uuid.
    let renderer_join_payload = json!({
        "session_uuid": session_uuid,
        "device_info": serde_json::to_value(&device_info).unwrap_or_default(),
        "is_active": true,
        "reason": join_reason,
        "initial_state": {
            "playing_state": PLAYING_STATE_STOPPED,
            "buffer_state": BUFFER_STATE_OK,
            "current_position": 0,
            "duration": 0,
            "queue_version": {
                "major": queue_version_ref.major,
                "minor": queue_version_ref.minor
            }
        }
    });
    let renderer_join_report = RendererReport::new(
        RendererReportType::RndrSrvrJoinSession,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        renderer_join_payload,
    );
    if let Err(err) = app.send_renderer_report_command(renderer_join_report).await {
        log::error!("[QConnect] Deferred renderer join failed: {err}");
        return;
    }

    // 2. Initial StateUpdated report. At join time (e.g. reconnect mid-playback)
    // we may already have a current track, so resolve the real duration + current/
    // next queue_item_ids instead of hardcoding nulls.
    let renderer = app.renderer_state_snapshot().await;
    let queue = app.queue_state_snapshot().await;
    let current_track_id = renderer.current_track.as_ref().map(|item| item.track_id);
    let (current_qid, next_qid, _) = current_track_id
        .map(|tid| {
            qconnect_app::queue_resolution::resolve_queue_item_ids_from_queue_state(&queue, tid)
        })
        .unwrap_or((None, None, None));
    let duration_secs = match current_track_id {
        Some(track_id) => runtime
            .core()
            .get_track(track_id)
            .await
            .map(|track| u64::from(track.duration))
            .unwrap_or(0),
        None => 0,
    };
    let mut state_report_payload = json!({
        "playing_state": PLAYING_STATE_STOPPED,
        "buffer_state": BUFFER_STATE_OK,
        "current_position": 0,
        "duration": duration_secs,
        "queue_version": {
            "major": queue_version_ref.major,
            "minor": queue_version_ref.minor
        }
    });
    if let Some(qid) = current_qid {
        state_report_payload["current_queue_item_id"] = json!(qid);
    }
    if let Some(qid) = next_qid {
        state_report_payload["next_queue_item_id"] = json!(qid);
    }
    let state_report = RendererReport::new(
        RendererReportType::RndrSrvrStateUpdated,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        state_report_payload,
    );
    if let Err(err) = app.send_renderer_report_command(state_report).await {
        log::error!("[QConnect] Deferred renderer state report failed: {err}");
    }

    // 3. Report volume and max audio quality.
    let volume_report = RendererReport::new(
        RendererReportType::RndrSrvrVolumeChanged,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        json!({ "volume": 100 }),
    );
    if let Err(err) = app.send_renderer_report_command(volume_report).await {
        log::error!("[QConnect] Deferred renderer volume report failed: {err}");
    }

    let max_quality_report = RendererReport::new(
        RendererReportType::RndrSrvrMaxAudioQualityChanged,
        Uuid::new_v4().to_string(),
        queue_version_ref,
        json!({ "max_audio_quality": AUDIO_QUALITY_HIRES_LEVEL2 }),
    );
    if let Err(err) = app.send_renderer_report_command(max_quality_report).await {
        log::error!("[QConnect] Deferred renderer max quality report failed: {err}");
    }

    log::info!("[QConnect] Deferred renderer join complete");

    // Re-request session state so the server sends an updated renderer list
    // (including ourselves). Without this, the UI may not see QBZ as a renderer
    // until the next reconnect cycle.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let refresh_command = app
        .build_queue_command(QueueCommandType::CtrlSrvrAskForQueueState, json!({}))
        .await;
    if let Ok(action_uuid) = app.send_queue_command(refresh_command).await {
        app.clear_pending_if_matches(&action_uuid).await;
        log::info!("[QConnect] Re-requested session state after renderer join");
    }

    // Resync the active renderer's full state too, so a reconnect rejoin restores
    // renderer state.
    if let Err(err) = app.ask_for_active_renderer_state().await {
        log::warn!("[QConnect] Post-join AskForRendererState failed: {err}");
    }

    // Record this session_uuid so a subsequent SESSION_STATE with the same uuid
    // takes the idempotent fast-path above.
    {
        let mut st = sync_state.lock().await;
        st.last_joined_session_uuid = Some(session_uuid.to_string());
    }
}
