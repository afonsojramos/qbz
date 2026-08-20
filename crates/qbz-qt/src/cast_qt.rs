//! Cast (Chromecast / DLNA) service — the Qt port of
//! `crates/qbz/src/cast_service.rs`, on top of the SHARED, frontend-agnostic
//! `qbz-cast` crate (ADR-006). Nothing about the protocols, the discovery or
//! the media server is reimplemented here: this module is the driver
//! (lifecycle + routing + state publishing), exactly like its Slint twin.
//!
//! Shape, 1:1 with the Slint service:
//!   - ONE process-wide singleton holding the two discovery handles, the
//!     ACTIVE connection (exactly one protocol at a time), one lazy shared
//!     `MediaServer` and a 1 s position poll;
//!   - discovery runs ONLY while the picker is open (`open_picker` /
//!     `close_picker`) — nothing scans in the background, nothing touches the
//!     network until the user asks;
//!   - casting BYPASSES the local audio backend entirely (the renderer's own
//!     DAC decodes the bytes we serve). The PROTECTED local playback path is
//!     never modified — it is only STOPPED while a renderer owns transport.
//!
//! Bytes + MIME are resolved through the shared core API
//! `fetch_for_external_stream_resolved` at the user's streaming-quality
//! preference clamped by the manual per-renderer cap (#638 fix 4). Caps govern
//! what we REQUEST only: bytes already in the L1/L2 cache go out as-is, never
//! resampled. The LOCAL output device's cap never applies here (the local DAC
//! is not in a cast's signal path — #638 precedence rule).
//!
//! SIZE (project rule): this file is one cohesive state machine — the inner
//! state, its lock discipline and the poll are not separable without leaking
//! `CastInner` across a module boundary. The Slint original is 1606 lines in
//! ONE file for the same reason; this is the trimmed port of it. The Qt
//! boundary (properties + invokables) IS split out, into `cast_bridge.rs`.
//!
//! QConnect coexistence (contract §11.4, 1:1 with cast_service.rs:280-294 /
//! :405-437 / :1140-1161): cast connect halts local playback, then SUSPENDS a
//! live QConnect session (best-effort — casting never blocks on it); cast
//! disconnect restores it. The golden bar badge is republished around the
//! suspend/restore because the facade deliberately leaves badge flips to its
//! callers (the toggle / startup auto-connect / offline watcher pattern).
//!
//! DELIBERATE CUTS vs the Slint service, each named:
//!   - no offline-cache tier: the Qt port never brings up `OfflineCacheState`
//!     (`settings_qt/offline.rs` says so), so the resolver is called with
//!     `None` — cache -> network still works, a download-only track does not;
//!   - no Plex casting: the Slint has the same TODO (needs the Plex bytes
//!     resolver);
//!   - no CAST-side lyrics anchor feed: the Slint cast poll publishes its
//!     position into the lyrics remote anchor (cast_service.rs:1086-1097) so
//!     lyrics auto-follow while casting; here `lyrics_qt::position_ms` reads
//!     the local player while casting (the QConnect anchor, §11.1, gates on
//!     `now_playing::is_remote()`, which is false during a cast — see GLUE in
//!     the report). The cast-disconnect `clear_remote_anchor`
//!     (cast_service.rs:439-440) is likewise subsumed: the QConnect suspend
//!     already cleared the anchor through `now_playing::set_remote(false)`.
//! ADDITION (asked for): a renderer that goes silent mid-session is torn down
//! after `LOST_POLL_MAX` consecutive failed reads instead of leaving the UI
//! claiming a live connection forever.

use std::sync::{Arc, OnceLock};

use qbz_app::shell::AppRuntime;
use qbz_cast::{
    CastPositionInfo, ChromecastHandle, DeviceDiscovery, DiscoveredDevice, DiscoveredDlnaDevice,
    DlnaConnection, DlnaDiscovery, DlnaMetadata, DlnaPositionInfo, MediaMetadata, MediaServer,
};
use qbz_core::LoggingAdapter;
use qbz_models::{probe_streaminfo, AssetOrigin, AudioParams, Quality, QualityLimit, QueueTrack};
use tokio::sync::Mutex;

use crate::cast_bridge;

type Runtime = Arc<AppRuntime<LoggingAdapter>>;

/// Cast position poll cadence (Tauri's `POSITION_POLL_INTERVAL_MS`).
const POSITION_POLL_INTERVAL_MS: u64 = 1000;
/// Picker device-list poll cadence + scan-spinner window (mirror Tauri).
const DEVICE_POLL_INTERVAL_MS: u64 = 2000;
const SCAN_DURATION_MS: u64 = 15000;

/// How close (seconds) the renderer must get to a track's end before a DLNA
/// `STOPPED` counts as a genuine track end rather than a renderer hiccup.
const CAST_END_GUARD_SECS: f64 = 5.0;
/// Below this observed max position the renderer's RelTime is unreliable
/// (plenty never implement GetPositionInfo) and the near-end guard is skipped.
const CAST_POSITION_SIGNAL_MIN_SECS: f64 = 1.0;
/// A guard must never wedge the queue: honor a persistent STOPPED anyway.
const CAST_PREMATURE_STOP_POLLS_MAX: u32 = 4;
/// Consecutive failed position reads before the session is declared lost.
const LOST_POLL_MAX: u32 = 5;

// ---------------------------------------------------------------------------
// Protocol tag
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CastProtocol {
    Chromecast,
    Dlna,
}

impl CastProtocol {
    fn as_str(self) -> &'static str {
        match self {
            CastProtocol::Chromecast => "chromecast",
            CastProtocol::Dlna => "dlna",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "chromecast" => Some(CastProtocol::Chromecast),
            "dlna" => Some(CastProtocol::Dlna),
            _ => None,
        }
    }
}

/// What `register_*` learned about the asset it just registered (#638 fix 1):
/// the MIME, the measured STREAMINFO probe of the bytes actually served
/// (None = non-FLAC / local file), where they came from, and the tier the
/// resolver was asked for (None = source not governed by the preference).
struct CastAssetInfo {
    content_type: String,
    probe: Option<AudioParams>,
    origin: Option<AssetOrigin>,
    requested: Option<Quality>,
    /// Request-time cause paired with `requested` (#638 fix 4).
    request_cause: QualityLimit,
}

// ---------------------------------------------------------------------------
// Module singleton
// ---------------------------------------------------------------------------

static SERVICE: OnceLock<Arc<CastService>> = OnceLock::new();

/// The process-wide cast service. Constructed on first use — every entry
/// point is user-driven (a picker invokable) or lives on the playback path,
/// both of which run after `APP` is set by the boot sequence.
pub(crate) fn service() -> Arc<CastService> {
    SERVICE
        .get_or_init(|| Arc::new(CastService::new(crate::app())))
        .clone()
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CastInner {
    // Discovery (alive only while the picker is open).
    chromecast_discovery: Option<DeviceDiscovery>,
    dlna_discovery: Option<DlnaDiscovery>,
    // Active connection (exactly one protocol at a time).
    chromecast: Option<ChromecastHandle>,
    dlna: Option<DlnaConnection>,
    protocol: Option<CastProtocol>,
    connected_device_ip: Option<String>,
    connected_device_name: Option<String>,
    // Stable identity + the ui_prefs cap key derived from it (#638 fix 4).
    // `connected_cap_key` is None when no persistable identity exists (a
    // Chromecast without the mDNS TXT `id` record — a fullname-keyed cap
    // would silently detach on rename); the picker hides the row for it.
    connected_device_id: Option<String>,
    connected_cap_key: Option<String>,
    // ONE shared lazy media server for both protocols.
    media_server: Option<MediaServer>,
    // Playback mirror. `current_track_id` is session bookkeeping (which track
    // the renderer currently holds) kept 1:1 with the Slint service; nothing
    // reads it yet, and the allow keeps that honest instead of dropping a
    // field the transport work will want.
    #[allow(dead_code)]
    current_track_id: Option<u64>,
    is_playing: bool,
    // Track-end one-shot latch (reset on PLAYING).
    track_end_detected: bool,
    // DLNA track-end guard state (all three reset per new track).
    cast_saw_playing: bool,
    cast_max_position: f64,
    cast_premature_stop_polls: u32,
    // Consecutive failed position reads (device-disappeared detection).
    lost_polls: u32,
    // QConnect coexistence (§11.4): whether QConnect was on before casting,
    // so `disconnect` restores exactly the sessions `connect` suspended.
    qconnect_was_on_before_cast: bool,
    // Position-poll task; aborted on disconnect.
    poll_task: Option<tokio::task::JoinHandle<()>>,
    // Device-refresh task (2 s loop while the picker is open).
    discovery_task: Option<tokio::task::JoinHandle<()>>,
}

pub(crate) struct CastService {
    inner: Arc<Mutex<CastInner>>,
    runtime: Runtime,
}

impl CastService {
    fn new(runtime: Runtime) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CastInner::default())),
            runtime,
        }
    }

    /// True while a renderer is connected and owns transport.
    pub(crate) async fn is_casting(&self) -> bool {
        self.inner.lock().await.protocol.is_some()
    }

    // ---- Discovery ---------------------------------------------------------

    /// Start mDNS (Chromecast) + SSDP (DLNA) discovery, the 2 s device-refresh
    /// loop and the 15 s scan-spinner window. Picker-owned: this is the ONLY
    /// place either discovery is armed.
    pub(crate) async fn start_discovery(self: &Arc<Self>) {
        {
            let mut inner = self.inner.lock().await;
            if inner.chromecast_discovery.is_none() {
                let mut disco = DeviceDiscovery::new();
                if let Err(e) = disco.start_discovery() {
                    log::warn!("[qbz-qt][Cast] chromecast discovery start failed: {e}");
                }
                inner.chromecast_discovery = Some(disco);
            }
            if inner.dlna_discovery.is_none() {
                let mut disco = DlnaDiscovery::new();
                if let Err(e) = disco.start_discovery().await {
                    log::warn!("[qbz-qt][Cast] dlna discovery start failed: {e}");
                }
                inner.dlna_discovery = Some(disco);
            }
        }

        // Arm the scan-spinner window.
        set_scanning(true);
        crate::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(SCAN_DURATION_MS)).await;
            set_scanning(false);
        });

        // 2 s device-refresh loop (replaces any prior).
        let svc = self.clone();
        let task = tokio::spawn(async move {
            loop {
                svc.refresh_devices().await;
                tokio::time::sleep(std::time::Duration::from_millis(DEVICE_POLL_INTERVAL_MS)).await;
            }
        });
        let mut inner = self.inner.lock().await;
        if let Some(old) = inner.discovery_task.replace(task) {
            old.abort();
        }
    }

    /// Stop both discoveries + the refresh loop (picker closed). The active
    /// connection is untouched — a cast survives the picker.
    pub(crate) async fn stop_discovery(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(task) = inner.discovery_task.take() {
            task.abort();
        }
        if let Some(mut disco) = inner.chromecast_discovery.take() {
            let _ = disco.stop_discovery();
        }
        if let Some(mut disco) = inner.dlna_discovery.take() {
            let _ = disco.stop_discovery();
        }
        set_scanning(false);
    }

    /// `(chromecast_count, dlna_count, [(protocol, name)])` for the Settings >
    /// Developer Diagnostics panel's Cast Discovery section.
    ///
    /// The reference reads the `CastState` Slint global on the UI thread
    /// (`crates/qbz/src/diagnostics.rs:253-270`) because that is where its
    /// picker's device list lives. Here the lists live in `inner`, so the
    /// panel reads them straight from the service instead of round-tripping
    /// through the bridge — same numbers, one fewer hop, and no oneshot.
    /// PURE read: it never starts or stops discovery.
    pub(crate) async fn diag_devices(&self) -> (i32, i32, Vec<(String, String)>) {
        let inner = self.inner.lock().await;
        let cc = inner
            .chromecast_discovery
            .as_ref()
            .map(|d| d.get_discovered_devices())
            .unwrap_or_default();
        let dl = inner
            .dlna_discovery
            .as_ref()
            .map(|d| d.get_discovered_devices())
            .unwrap_or_default();
        let mut rows: Vec<(String, String)> = Vec::with_capacity(cc.len() + dl.len());
        for d in &cc {
            rows.push(("chromecast".to_string(), d.name.clone()));
        }
        for d in &dl {
            rows.push(("dlna".to_string(), d.name.clone()));
        }
        (cc.len() as i32, dl.len() as i32, rows)
    }

    /// Snapshot both device lists and push them to the picker.
    async fn refresh_devices(&self) {
        let (chromecast, dlna) = {
            let inner = self.inner.lock().await;
            let cc = inner
                .chromecast_discovery
                .as_ref()
                .map(|d| d.get_discovered_devices())
                .unwrap_or_default();
            let dl = inner
                .dlna_discovery
                .as_ref()
                .map(|d| d.get_discovered_devices())
                .unwrap_or_default();
            (cc, dl)
        };
        push_devices(chromecast, dlna);
    }

    // ---- Connect / disconnect ----------------------------------------------

    /// Connect to a device: halt local playback, suspend QConnect if it was
    /// on (§11.4), then re-cast the current track at its position if one was
    /// playing (`castStore.connectToDevice`).
    pub(crate) async fn connect(
        self: &Arc<Self>,
        device_id: String,
        protocol: String,
    ) -> Result<(), String> {
        let proto = CastProtocol::from_str(&protocol)
            .ok_or_else(|| format!("Unknown cast protocol: {protocol}"))?;

        // Snapshot local playback BEFORE we tear it down.
        let snapshot_track = self.runtime.core().current_track().await;
        let pb = self.runtime.core().get_playback_state();
        let was_playing = pb.is_playing;
        let resume_pos = pb.position;

        // Halt the local audio backend (no double audio). ENTERING the
        // protected path's public seam only — nothing about it changes.
        let _ = self.runtime.core().stop();

        // Suspend QConnect if it was on (§11.4 — best-effort; NEVER blocks
        // casting). Same ordering as the reference (cast_service.rs:293-294):
        // after the local halt, before the renderer connect.
        self.suspend_qconnect_if_on().await;

        // Connect to the renderer.
        let device_ip = match proto {
            CastProtocol::Chromecast => self.connect_chromecast(&device_id).await?,
            CastProtocol::Dlna => self.connect_dlna(&device_id).await?,
        };

        {
            let mut inner = self.inner.lock().await;
            inner.protocol = Some(proto);
            inner.connected_device_ip = Some(device_ip);
            inner.track_end_detected = false;
            inner.cast_saw_playing = false;
            inner.cast_max_position = 0.0;
            inner.cast_premature_stop_polls = 0;
            inner.lost_polls = 0;
            log::info!(
                "[qbz-qt][Cast] connected to {} ({}; cap key: {})",
                inner.connected_device_name.as_deref().unwrap_or("?"),
                inner.connected_device_id.as_deref().unwrap_or("?"),
                inner
                    .connected_cap_key
                    .as_deref()
                    .unwrap_or("none — unstable id"),
            );
        }
        set_error(String::new());
        self.push_connection_state().await;
        self.push_device_cap_row().await;
        self.start_position_poll();

        // Re-cast the current track at its position, passing the REAL source.
        if was_playing {
            if let Some(track) = snapshot_track {
                if let Err(e) = self.cast_track(&track).await {
                    log::warn!("[qbz-qt][Cast] resume re-cast failed: {e}");
                    set_error(e);
                } else if resume_pos > 5 {
                    // Deferred seek (the renderer needs the media loaded first).
                    let svc = self.clone();
                    let pos = resume_pos as f64;
                    crate::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        let _ = svc.seek_secs(pos).await;
                    });
                }
            }
        }
        Ok(())
    }

    async fn connect_chromecast(&self, device_id: &str) -> Result<String, String> {
        let device: DiscoveredDevice = {
            let inner = self.inner.lock().await;
            inner
                .chromecast_discovery
                .as_ref()
                .and_then(|d| d.get_device(device_id))
                .ok_or_else(|| format!("Chromecast device not found: {device_id}"))?
        };
        let handle = ChromecastHandle::new();
        handle
            .connect(device.ip.clone(), device.port)
            .map_err(|e| e.to_string())?;
        let mut inner = self.inner.lock().await;
        inner.chromecast = Some(handle);
        inner.connected_device_name = Some(device.name.clone());
        // Cap key only when the id is the mDNS TXT `id` record (the Cast
        // UUID). The fullname fallback tracks the friendly name, so a cap
        // keyed on it would silently stop applying on rename.
        inner.connected_cap_key = device
            .id_is_stable
            .then(|| format!("chromecast:{}", device.id));
        if !device.id_is_stable {
            log::info!(
                "[qbz-qt][Cast] {} broadcasts no mDNS TXT id — per-device quality cap unavailable",
                device.name
            );
        }
        inner.connected_device_id = Some(device.id);
        Ok(device.ip)
    }

    async fn connect_dlna(&self, device_id: &str) -> Result<String, String> {
        // `DlnaConnection::connect` consumes the discovered device by value.
        let device: DiscoveredDlnaDevice = {
            let inner = self.inner.lock().await;
            inner
                .dlna_discovery
                .as_ref()
                .and_then(|d| d.get_device(device_id))
                .ok_or_else(|| format!("DLNA device not found: {device_id}"))?
        };
        let ip = device.ip.clone();
        let name = device.name.clone();
        let udn = device.id.clone();
        let conn = DlnaConnection::connect(device)
            .await
            .map_err(|e| e.to_string())?;
        let mut inner = self.inner.lock().await;
        inner.dlna = Some(conn);
        inner.connected_device_name = Some(name);
        // The DLNA id IS the UPnP UDN — stable by construction, so a DLNA
        // renderer is always cappable.
        inner.connected_cap_key = Some(format!("dlna:{udn}"));
        inner.connected_device_id = Some(udn);
        Ok(ip)
    }

    /// Disconnect: stop the renderer, drop the connection, restore the
    /// QConnect session connect() suspended (§11.4), reset state.
    pub(crate) async fn disconnect(&self) {
        // Stop the renderer first (disconnect alone leaves it playing).
        let _ = self.stop_renderer().await;

        let (poll, was_on) = {
            let mut inner = self.inner.lock().await;
            if let Some(h) = inner.chromecast.take() {
                let _ = h.disconnect();
            }
            if let Some(mut c) = inner.dlna.take() {
                let _ = c.disconnect();
            }
            inner.protocol = None;
            inner.connected_device_ip = None;
            inner.connected_device_name = None;
            inner.connected_device_id = None;
            inner.connected_cap_key = None;
            // Release the served track buffers with the session (#550); the
            // server itself stays up for the next connect.
            if let Some(server) = inner.media_server.as_ref() {
                server.clear_entries();
            }
            inner.current_track_id = None;
            inner.is_playing = false;
            inner.track_end_detected = false;
            inner.lost_polls = 0;
            (inner.poll_task.take(), inner.qconnect_was_on_before_cast)
        };
        if let Some(task) = poll {
            task.abort();
        }
        // Restore the QConnect session connect() suspended (best-effort),
        // then reset the latch (cast_service.rs:435-438).
        if was_on {
            self.restore_qconnect().await;
            self.inner.lock().await.qconnect_was_on_before_cast = false;
        }
        // Clear the per-connection disclosure + cap row.
        cast_bridge::ui(|mut b| {
            b.as_mut().set_quality_limit_cause(0);
            b.as_mut().set_quality_over_cap(false);
            b.as_mut()
                .set_quality_origin(cxx_qt_lib::QString::from(""));
            b.as_mut().set_device_cap_available(false);
            b.as_mut()
                .set_device_cap_key(cxx_qt_lib::QString::from(""));
            b.as_mut().set_device_cap_index(0);
        });
        self.push_connection_state().await;
    }

    // ---- QConnect coexistence (§11.4 — cast_service.rs:1140-1161) -----------

    /// Suspend QConnect while casting (mutual exclusion). Best-effort: a
    /// failure logs and casting proceeds. The latch is recorded ONLY when a
    /// session was actually live, so `disconnect` restores exactly what this
    /// suspended.
    async fn suspend_qconnect_if_on(&self) {
        let Some(qc) = crate::qconnect_qt::service() else {
            return;
        };
        if !qc.is_running().await {
            return;
        }
        self.inner.lock().await.qconnect_was_on_before_cast = true;
        if let Err(e) = qc.disconnect().await {
            log::warn!("[qbz-qt][Cast] QConnect suspend failed (continuing): {e}");
        }
        // The facade deliberately does NOT flip the bar badge itself (the
        // toggle / startup auto-connect / offline force-disconnect paths each
        // publish their own — the qconnect_bridge.rs connectToggle tail); a
        // suspend must not leave the golden button lit while the session is
        // down. The facade's disconnect always tears the runtime down, so the
        // badge goes dark even when the call above logged an error.
        crate::qconnect_qt::publish::connected(false);
    }

    /// Bring the suspended session back after casting (best-effort). The
    /// badge only re-lights when the session is actually live again.
    async fn restore_qconnect(&self) {
        let Some(qc) = crate::qconnect_qt::service() else {
            return;
        };
        match qc.connect().await {
            Ok(()) => crate::qconnect_qt::publish::connected(true),
            Err(e) => log::warn!("[qbz-qt][Cast] QConnect restore failed: {e}"),
        }
    }

    // ---- Casting a track ----------------------------------------------------

    /// Resolve a track's bytes + MIME, register them with the shared media
    /// server, and hand the URL to the active renderer. Routes by source.
    pub(crate) async fn cast_track(self: &Arc<Self>, track: &QueueTrack) -> Result<(), String> {
        let proto = {
            let inner = self.inner.lock().await;
            inner.protocol.ok_or_else(|| "Not connected".to_string())?
        };

        let source = if track.is_local {
            "local"
        } else {
            track.source.as_deref().unwrap_or("qobuz")
        };

        // Resolve + register per source. The fetch happens OUTSIDE the lock.
        let info = match source {
            "local" | "ephemeral" => {
                let path = resolve_castable_path(track).await?;
                self.register_local(track.id, &path).await?
            }
            "qobuz" | "qobuz_download" => self.register_qobuz(track.id).await?,
            "plex" => {
                // TODO(cast-plex): needs a proxy that re-serves the resolved
                // part bytes to the renderer. `PlaybackTicket::Stream` now
                // hands over the url that proxy would read, so what is missing
                // is the media-server arm, not the resolve. The Slint service
                // carries the same TODO.
                //
                // Kept as an EARLY refusal on purpose: `resolve_castable_path`
                // would refuse it too — structurally, for every remote source,
                // including ones that have no arm here — but only after paying
                // a network round trip to build a url it is about to throw
                // away.
                return Err("Plex casting is not yet supported".to_string());
            }
            other => return Err(format!("Unsupported cast source: {other}")),
        };
        let content_type = info.content_type.clone();

        // Build the per-device URL and hand it to the renderer.
        let url = {
            let inner = self.inner.lock().await;
            let ip = inner.connected_device_ip.clone();
            let server = inner
                .media_server
                .as_ref()
                .ok_or_else(|| "Media server not initialized".to_string())?;
            match ip.as_deref() {
                Some(ip) => server.get_audio_url_for_target(track.id, ip),
                None => server.get_audio_url(track.id),
            }
            .ok_or_else(|| "Failed to build media URL".to_string())?
        };

        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                let handle = inner
                    .chromecast
                    .as_ref()
                    .ok_or("Chromecast not connected")?;
                // load_media auto-plays on the Default Media Receiver.
                handle
                    .load_media(url, content_type, media_metadata(track))
                    .map_err(|e| e.to_string())?;
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                let conn = inner.dlna.as_mut().ok_or("DLNA not connected")?;
                // DLNA is a TWO-step load -> play.
                let result = async {
                    conn.load_media(&url, &dlna_metadata(track), &content_type)
                        .await
                        .map_err(|e| e.to_string())?;
                    conn.play().await.map_err(|e| e.to_string())?;
                    Ok::<(), String>(())
                }
                .await;
                if let Err(e) = result {
                    // Best-effort reset so the renderer doesn't sit half-loaded
                    // on a URI it already faulted (#646).
                    let _ = conn.stop().await;
                    return Err(e);
                }
            }
        }

        {
            let mut inner = self.inner.lock().await;
            inner.current_track_id = Some(track.id);
            inner.is_playing = true;
            inner.track_end_detected = false;
            inner.cast_saw_playing = false;
            inner.cast_max_position = 0.0;
            inner.cast_premature_stop_polls = 0;
            inner.lost_polls = 0;
        }
        // Delivered quality for the picker line (#638 fix 1): MEASURED from
        // the served bytes when the probe can read them, catalog fallback
        // otherwise (non-FLAC / local files).
        let (quality_label, quality_detail) = match info.probe {
            Some(p) => (
                if p.bits_per_sample >= 24 {
                    "Hi-Res FLAC"
                } else {
                    "FLAC"
                }
                .to_string(),
                crate::quality_state::detail(
                    Some(p.bits_per_sample),
                    Some(p.sample_rate as f64),
                ),
            ),
            None => quality_label_from_track(track),
        };
        push_quality(quality_label, quality_detail);
        // Un-stale the now-playing stamp + disclose over-cap serves: the local
        // poll (which normally owns those properties) reports a stopped engine
        // while casting.
        self.publish_measured_badge(&info).await;
        set_error(String::new());
        self.push_connection_state().await;
        Ok(())
    }

    /// qobuz: resolve via the shared core API (cache -> network here; the Qt
    /// port has no offline store), probe the served bytes, register them.
    async fn register_qobuz(&self, track_id: u64) -> Result<CastAssetInfo, String> {
        // The streaming preference — clamped by THIS renderer's manual cap
        // (#638 fix 4) — governs what we REQUEST, resolved fresh per cast
        // track so a Settings or cap change applies to the very next one.
        let cap_key = self.inner.lock().await.connected_cap_key.clone();
        let (quality, request_cause) = effective_cast_quality(cap_key.as_deref());
        let asset = self
            .runtime
            .core()
            // No offline tier / cache sink: `OfflineCacheState` is not wired
            // in the Qt port (see the module header).
            .fetch_for_external_stream_resolved(track_id, quality, None, None)
            .await
            .ok_or_else(|| format!("Could not resolve stream for track {track_id}"))?;

        log::info!(
            "[qbz-qt][Cast] qobuz track {track_id} resolved from {:?}",
            asset.origin
        );
        let content_type = asset.content_type.clone();
        // Measure BEFORE register_audio moves the bytes.
        let probe = probe_streaminfo(&asset.bytes);
        let origin = asset.origin;

        self.ensure_media_server().await?;
        {
            let mut inner = self.inner.lock().await;
            let server = inner.media_server.as_mut().ok_or("Media server gone")?;
            server.register_audio(track_id, asset.bytes, &content_type);
        }
        Ok(CastAssetInfo {
            content_type,
            probe,
            origin: Some(origin),
            requested: Some(quality),
            request_cause,
        })
    }

    /// local: stream the file from disk via register_file (no full-RAM read).
    /// No probe/origin/requested tier — local files are not governed by the
    /// streaming preference and keep the catalog-metadata fallback.
    async fn register_local(&self, track_id: u64, path: &str) -> Result<CastAssetInfo, String> {
        self.ensure_media_server().await?;
        let content_type = {
            let mut inner = self.inner.lock().await;
            let server = inner.media_server.as_mut().ok_or("Media server gone")?;
            server
                .register_file(track_id, path)
                .map_err(|e| e.to_string())?;
            content_type_for_local(path)
        };
        Ok(CastAssetInfo {
            content_type,
            probe: None,
            origin: None,
            requested: None,
            request_cause: QualityLimit::None,
        })
    }

    async fn ensure_media_server(&self) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        if inner.media_server.is_none() {
            let server = MediaServer::start().map_err(|e| e.to_string())?;
            inner.media_server = Some(server);
        }
        Ok(())
    }

    // ---- Transport (cast-first gating) --------------------------------------
    //
    // Each returns Ok(false) = "not casting, fall through to the local path",
    // Ok(true) = handled. Call sites live on the playback path — see the
    // report's GLUE NEEDED for `playback_qt.rs`.

    #[allow(dead_code)] // wired by playback_qt::toggle_play — see GLUE
    pub(crate) async fn toggle_play_if_cast(&self) -> Result<bool, String> {
        let (proto, playing) = {
            let inner = self.inner.lock().await;
            match inner.protocol {
                Some(p) => (p, inner.is_playing),
                None => return Ok(false),
            }
        };
        if playing {
            self.pause_renderer(proto).await?;
        } else {
            self.play_renderer(proto).await?;
        }
        self.inner.lock().await.is_playing = !playing;
        self.push_connection_state().await;
        Ok(true)
    }

    /// Seek to a 0..1 fraction of the CURRENT cast track. The seekbar cannot
    /// derive the absolute position from the local engine while casting (it
    /// is stopped, so its duration reads 0 and every drag would restart the
    /// track) — resolve the duration from the catalog metadata instead.
    #[allow(dead_code)] // wired by playback_qt::seek_frac — see GLUE
    pub(crate) async fn seek_fraction_if_cast(&self, fraction: f64) -> Result<bool, String> {
        if !self.is_casting().await {
            return Ok(false);
        }
        let dur = self
            .runtime
            .core()
            .current_track()
            .await
            .map(|t| t.duration_secs as f64)
            .unwrap_or(0.0);
        if dur <= 0.0 {
            // No usable duration — swallow the seek rather than jump to 0.
            return Ok(true);
        }
        let secs = (fraction.clamp(0.0, 1.0) * dur).max(0.0);
        self.seek_secs(secs).await?;
        Ok(true)
    }

    #[allow(dead_code)] // wired by playback_qt::set_volume — see GLUE
    pub(crate) async fn set_volume_if_cast(&self, volume: f32) -> Result<bool, String> {
        let proto = {
            let inner = self.inner.lock().await;
            match inner.protocol {
                Some(p) => p,
                None => return Ok(false),
            }
        };
        let v = volume.clamp(0.0, 1.0);
        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                if let Some(h) = inner.chromecast.as_ref() {
                    h.set_volume(v).map_err(|e| e.to_string())?;
                }
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                if let Some(c) = inner.dlna.as_mut() {
                    c.set_volume(v).await.map_err(|e| e.to_string())?;
                }
            }
        }
        // Reflect the drag on the bar: the local set_volume is skipped while
        // casting and the cast poll doesn't push volume.
        crate::now_playing::set_volume(v);
        Ok(true)
    }

    // NOTE: next/previous are intentionally NOT gated. While casting, the
    // local advance flow still runs (it moves the core cursor + refreshes the
    // card/queue) and the play route casts the new current track. A cast-only
    // advance would desync the UI cursor from the renderer.

    async fn seek_secs(&self, secs: f64) -> Result<(), String> {
        let proto = {
            let inner = self.inner.lock().await;
            match inner.protocol {
                Some(p) => p,
                None => return Ok(()),
            }
        };
        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                if let Some(h) = inner.chromecast.as_ref() {
                    h.seek(secs).map_err(|e| e.to_string())?;
                }
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                if let Some(c) = inner.dlna.as_mut() {
                    c.seek(secs.max(0.0) as u64)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    #[allow(dead_code)] // only reached through toggle_play_if_cast — see GLUE
    async fn play_renderer(&self, proto: CastProtocol) -> Result<(), String> {
        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                if let Some(h) = inner.chromecast.as_ref() {
                    h.play().map_err(|e| e.to_string())?;
                }
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                if let Some(c) = inner.dlna.as_mut() {
                    c.play().await.map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    #[allow(dead_code)] // only reached through toggle_play_if_cast — see GLUE
    async fn pause_renderer(&self, proto: CastProtocol) -> Result<(), String> {
        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                if let Some(h) = inner.chromecast.as_ref() {
                    h.pause().map_err(|e| e.to_string())?;
                }
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                if let Some(c) = inner.dlna.as_mut() {
                    c.pause().await.map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    async fn stop_renderer(&self) -> Result<(), String> {
        let proto = {
            let inner = self.inner.lock().await;
            match inner.protocol {
                Some(p) => p,
                None => return Ok(()),
            }
        };
        match proto {
            CastProtocol::Chromecast => {
                let inner = self.inner.lock().await;
                if let Some(h) = inner.chromecast.as_ref() {
                    h.stop().map_err(|e| e.to_string())?;
                }
            }
            CastProtocol::Dlna => {
                let mut inner = self.inner.lock().await;
                if let Some(c) = inner.dlna.as_mut() {
                    c.stop().await.map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }

    // ---- Position poll + ended detection ------------------------------------

    fn start_position_poll(self: &Arc<Self>) {
        let svc = self.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(POSITION_POLL_INTERVAL_MS))
                    .await;
                if !svc.is_casting().await {
                    break;
                }
                svc.poll_once().await;
            }
        });
        let svc2 = self.clone();
        crate::spawn(async move {
            let mut inner = svc2.inner.lock().await;
            if let Some(old) = inner.poll_task.replace(task) {
                old.abort();
            }
        });
    }

    async fn poll_once(self: &Arc<Self>) {
        let proto = {
            let inner = self.inner.lock().await;
            match inner.protocol {
                Some(p) => p,
                None => return,
            }
        };

        // Read position/state from the active renderer.
        let read = match proto {
            CastProtocol::Chromecast => {
                let info: Option<CastPositionInfo> = {
                    let inner = self.inner.lock().await;
                    inner
                        .chromecast
                        .as_ref()
                        .and_then(|h| h.get_media_position().ok())
                };
                info.map(|i| {
                    let st = i.player_state.to_uppercase();
                    let playing = st == "PLAYING";
                    (i.position_secs, i.duration_secs, st, playing)
                })
            }
            CastProtocol::Dlna => {
                let info: Option<DlnaPositionInfo> = {
                    let inner = self.inner.lock().await;
                    match inner.dlna.as_ref() {
                        Some(c) => c.get_position_info().await.ok(),
                        None => None,
                    }
                };
                info.map(|i| {
                    let st = i.transport_state.to_uppercase();
                    let playing = st == "PLAYING";
                    (i.position_secs as f64, i.duration_secs as f64, st, playing)
                })
            }
        };

        // DEVICE DISAPPEARED (the one addition over the Slint service, which
        // simply skips the tick): a renderer that has been unplugged /
        // rebooted / left the network answers nothing. Skipping forever would
        // leave the picker claiming a live connection and the bar frozen, so
        // after LOST_POLL_MAX consecutive failures the session is torn down
        // and the raw reason is surfaced on the picker's error line.
        let Some((position, duration, state, playing)) = read else {
            let lost = {
                let mut inner = self.inner.lock().await;
                inner.lost_polls += 1;
                inner.lost_polls
            };
            if lost >= LOST_POLL_MAX {
                let name = self
                    .inner
                    .lock()
                    .await
                    .connected_device_name
                    .clone()
                    .unwrap_or_default();
                log::warn!(
                    "[qbz-qt][Cast] {name} stopped answering after {lost} polls — dropping the session"
                );
                // Raw diagnostic, same register as the crate's connect errors
                // (`CastState.error` is a raw string in the Slint too).
                set_error(format!("Lost connection to {name}"));
                // Tear down from ANOTHER task on purpose: `disconnect` aborts
                // the poll task, and this IS the poll task — awaiting it here
                // would cancel `disconnect` mid-way (at its first await after
                // the abort) and leave the UI still claiming a connection.
                let svc = self.clone();
                crate::spawn(async move {
                    svc.disconnect().await;
                });
            } else {
                log::debug!("[qbz-qt][Cast] position read failed ({lost}/{LOST_POLL_MAX})");
            }
            return;
        };
        self.inner.lock().await.lost_polls = 0;

        // Many DLNA renderers report TrackDuration as 0 / NOT_IMPLEMENTED,
        // which left the seekbar permanently full. Fall back to the track's
        // catalog duration (the renderer's position stays authoritative).
        let duration = if duration > 0.0 {
            duration
        } else {
            self.runtime
                .core()
                .current_track()
                .await
                .map(|t| t.duration_secs as f64)
                .unwrap_or(0.0)
        };

        // Track-end detection: Chromecast {PLAYING,BUFFERING} -> IDLE; DLNA
        // PLAYING -> {STOPPED, NO_MEDIA_PRESENT}. One-shot latch, reset on
        // PLAYING. For DLNA a bare STOPPED is ambiguous (a renderer that
        // hiccups mid-track also reports STOPPED), so it only counts as
        // end-of-track when the track actually reached near its end —
        // guarded by the max position observed while PLAYING.
        let max_position;
        let ended = {
            let mut inner = self.inner.lock().await;
            inner.is_playing = playing;
            if state == "PLAYING" {
                inner.cast_saw_playing = true;
                inner.cast_max_position = inner.cast_max_position.max(position);
            }
            max_position = inner.cast_max_position;
            let ended = match proto {
                CastProtocol::Chromecast => state == "IDLE" && !inner.track_end_detected,
                CastProtocol::Dlna => {
                    let stopped = matches!(state.as_str(), "STOPPED" | "NO_MEDIA_PRESENT");
                    // The guard only makes sense when the position signal is
                    // usable: renderers whose RelTime never moves honor
                    // STOPPED like pre-guard behavior.
                    let position_reliable =
                        inner.cast_max_position > CAST_POSITION_SIGNAL_MIN_SECS;
                    let near_end = duration <= 0.0
                        || !position_reliable
                        || max_position >= duration - CAST_END_GUARD_SECS;
                    if stopped && inner.cast_saw_playing && !near_end {
                        inner.cast_premature_stop_polls += 1;
                        log::warn!(
                            "[qbz-qt][Cast] premature STOPPED {}/{} — not advancing yet \
                             (state={state}, max_position={max_position:.1}, \
                             duration={duration:.1})",
                            inner.cast_premature_stop_polls,
                            CAST_PREMATURE_STOP_POLLS_MAX
                        );
                    } else if !stopped {
                        inner.cast_premature_stop_polls = 0;
                    }
                    let persistent_stop =
                        inner.cast_premature_stop_polls >= CAST_PREMATURE_STOP_POLLS_MAX;
                    stopped
                        && inner.cast_saw_playing
                        && (near_end || persistent_stop)
                        && !inner.track_end_detected
                }
            };
            if state == "PLAYING" {
                inner.track_end_detected = false;
            } else if ended {
                inner.track_end_detected = true;
            }
            ended
        };

        log::debug!(
            "[qbz-qt][Cast] poll: state={state} position={position:.1} duration={duration:.1} \
             max_position={max_position:.1}"
        );

        // A paused renderer must park the FFT producer like a local pause does
        // (the local poll, which normally owns this, sees a stopped engine).
        crate::viz_qt::set_paused(!playing);

        // The cast poll drives the bar while connected.
        crate::now_playing::set_position(
            position as i32,
            duration as i32,
            playing,
            0.0,
            true,
        );
        push_position(position as f32, duration as f32, playing);

        if ended {
            log::info!(
                "[qbz-qt][Cast] track ended (state={state}, position={position:.1}, \
                 duration={duration:.1}, max_position={max_position:.1}); auto-advancing"
            );
            self.advance().await;
        }
    }

    /// End-of-track advance while casting. Moves the core cursor + refreshes
    /// the card/queue exactly like the local advance, then casts the new
    /// current track instead of opening a local stream.
    async fn advance(self: &Arc<Self>) {
        let runtime = self.runtime.clone();
        let Some(track) = runtime.core().next_track().await else {
            log::info!("[qbz-qt][Cast] queue finished");
            crate::now_playing::set_playing(false);
            let mut inner = self.inner.lock().await;
            inner.is_playing = false;
            return;
        };
        crate::playback_qt::refresh_now_playing(&runtime).await;
        crate::playback_qt::publish_queue(&runtime).await;
        if let Err(e) = self.cast_track(&track).await {
            log::warn!("[qbz-qt][Cast] advance to {} failed: {e}", track.id);
            set_error(e);
        }
    }

    // ---- Shutdown (logout / app exit) ---------------------------------------

    /// Tear everything down: stop the renderer, abort the poll, drop discovery
    /// and the media server, so a cast device does not keep playing after
    /// logout or exit (Tauri parity, #32/#33).
    #[allow(dead_code)] // wired by do_logout / the close handler — see GLUE
    pub(crate) async fn shutdown(&self) {
        let _ = self.stop_renderer().await;
        let mut inner = self.inner.lock().await;
        if let Some(task) = inner.poll_task.take() {
            task.abort();
        }
        if let Some(task) = inner.discovery_task.take() {
            task.abort();
        }
        if let Some(h) = inner.chromecast.take() {
            let _ = h.disconnect();
        }
        if let Some(mut c) = inner.dlna.take() {
            let _ = c.disconnect();
        }
        if let Some(mut disco) = inner.chromecast_discovery.take() {
            let _ = disco.stop_discovery();
        }
        if let Some(mut disco) = inner.dlna_discovery.take() {
            let _ = disco.stop_discovery();
        }
        if let Some(mut server) = inner.media_server.take() {
            server.stop();
        }
        inner.protocol = None;
        inner.connected_device_ip = None;
        inner.connected_device_name = None;
        inner.connected_device_id = None;
        inner.connected_cap_key = None;
        inner.current_track_id = None;
        inner.is_playing = false;
    }

    // ---- State push to the UI -----------------------------------------------

    async fn push_connection_state(&self) {
        let (connected, protocol, name, playing) = {
            let inner = self.inner.lock().await;
            (
                inner.protocol.is_some(),
                inner
                    .protocol
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default(),
                inner.connected_device_name.clone().unwrap_or_default(),
                inner.is_playing,
            )
        };
        if connected {
            crate::viz_qt::set_paused(!playing);
        }
        // The now-playing flags the bars already render: np_cast_active,
        // np_cast_protocol and np_cast_target all come from this ONE call.
        //
        // np_is_remote is deliberately NOT touched — it means "a Qobuz Connect
        // PEER owns the transport", which is a different thing, and the stamps
        // discriminate on exactly that: SongCardStamp.qml:284 and
        // AudioStamp.qml:82 gate the purple CAST/DLNA badge on
        // `npCastActive && !npIsRemote`, so raising is_remote here would HIDE
        // the badge this wiring exists to light. In the Slint the flag is
        // written only by `qconnect_event_sink.rs`; same split here.
        crate::now_playing::set_cast_session(connected, &protocol, &name);
        if connected {
            crate::now_playing::set_playing(playing);
        }

        let (p, n) = (protocol, name);
        cast_bridge::ui(move |mut b| {
            b.as_mut().set_connected(connected);
            b.as_mut()
                .set_protocol(cxx_qt_lib::QString::from(p.as_str()));
            b.as_mut()
                .set_device_name(cxx_qt_lib::QString::from(n.as_str()));
            b.as_mut().set_is_playing(playing);
        });
    }

    /// Publish the measured delivered-quality state the SKIPPED local poll
    /// would have published (#638 fix 1, cast half), plus the over-cap
    /// disclosure for the picker line — bytes that already existed locally
    /// are served AS-IS even above the requested tier, never resampled and
    /// never re-fetched, and the UI says so instead of hiding it.
    async fn publish_measured_badge(&self, info: &CastAssetInfo) {
        let (eff_rate_hz, eff_bits) = info
            .probe
            .map(|p| (p.sample_rate, p.bits_per_sample))
            .unwrap_or((0, 0));
        // `quality_state` holds the catalog max seeded by
        // `playback_qt::refresh_now_playing` for this same track.
        let delivered = crate::quality_state::evaluate(eff_rate_hz, eff_bits);
        // The cast's own request-time cause REPLACES the local one: the local
        // DAC cap is never a cast cause (#638 precedence rule), and only this
        // path knows whether the renderer cap shaped the request.
        let requested_id = info.requested.map(|q| q.id()).unwrap_or(0);
        let limit_cause = crate::quality_state::classify_limit_cause(
            delivered.downgraded,
            requested_id,
            info.request_cause as i32,
            eff_bits,
        );
        // Tier token for the stamp's main line. The shared helper's
        // requested-mp3 shortcut is for the LOCAL engine path; here a
        // successful STREAMINFO parse proves the bytes are FLAC, never MP3.
        let effective_tier = if delivered.downgraded && info.probe.is_some() {
            if eff_bits >= 24 {
                "hires"
            } else {
                "cd"
            }
            .to_string()
        } else {
            delivered.effective_tier.clone()
        };
        crate::now_playing::set_cast_delivered(crate::quality_state::Delivered {
            downgraded: delivered.downgraded,
            true_detail: delivered.true_detail,
            limit_cause,
            effective_tier,
        });

        // Over-cap: locally-existing bytes ABOVE the requested tier.
        let measured_tier = info.probe.map(|p| {
            if p.bits_per_sample >= 24 && p.sample_rate > 96_000 {
                Quality::UltraHiRes
            } else if p.bits_per_sample >= 24 {
                Quality::HiRes
            } else {
                Quality::Lossless
            }
        });
        let over_cap = matches!(info.origin, Some(o) if o != AssetOrigin::Network)
            && matches!((measured_tier, info.requested), (Some(m), Some(r)) if m > r);
        let origin_str = match info.origin {
            Some(AssetOrigin::Cache) => "cache",
            Some(AssetOrigin::Offline) => "offline",
            _ => "",
        };
        if over_cap {
            log::info!(
                "[qbz-qt][Cast] serving {origin_str} bytes above the requested tier \
                 (measured {measured_tier:?} > requested {:?}) — caps govern requests \
                 only; local bytes go out as-is, never resampled",
                info.requested
            );
        }
        cast_bridge::ui(move |mut b| {
            b.as_mut().set_quality_limit_cause(limit_cause);
            b.as_mut().set_quality_over_cap(over_cap);
            b.as_mut()
                .set_quality_origin(cxx_qt_lib::QString::from(origin_str));
        });
    }

    /// Push the per-renderer cap row for the connected device (#638 fix 4):
    /// whether a cap can be offered at all (stable identity only), the
    /// ui_prefs key the picker hands back, and the current index (0 = follow
    /// the app setting — the absent-entry default).
    async fn push_device_cap_row(&self) {
        let cap_key = self.inner.lock().await.connected_cap_key.clone();
        let index = cap_key.as_deref().map(cap_index_for_key).unwrap_or(0);
        cast_bridge::ui(move |mut b| {
            b.as_mut().set_device_cap_available(cap_key.is_some());
            b.as_mut().set_device_cap_key(cxx_qt_lib::QString::from(
                cap_key.unwrap_or_default().as_str(),
            ));
            b.as_mut().set_device_cap_index(index);
        });
    }

    /// Persist the user's manual cap choice for `cap_key` (#638 fix 4): index
    /// 0 removes the entry (follow the app setting), 1/2/3 store
    /// hires/cd/mp3. Enforcement is REQUEST-TIME only — the next cast resolve
    /// picks the change up; nothing is cleared, re-fetched or restarted (a
    /// per-device cap must never punish the global cache or break an
    /// in-flight cast).
    pub(crate) async fn set_device_cap(&self, cap_key: String, index: i32) {
        if cap_key.is_empty() {
            return;
        }
        let tier = match index {
            1 => Some("hires"),
            2 => Some("cd"),
            3 => Some("mp3"),
            _ => None,
        };
        let name = {
            let inner = self.inner.lock().await;
            inner.connected_device_name.clone().unwrap_or_default()
        };
        let mut caps = cast_caps();
        match tier {
            Some(t) => {
                caps.insert(
                    cap_key.clone(),
                    serde_json::json!({ "tier": t, "name": name }),
                );
                log::info!("[qbz-qt][Cast] quality cap for {name} ({cap_key}) -> {t}");
            }
            None => {
                caps.remove(&cap_key);
                log::info!(
                    "[qbz-qt][Cast] quality cap for {name} ({cap_key}) removed — follows the app setting"
                );
            }
        }
        // Additive single-key patch of the SHARED ui_prefs.json — the same
        // file and the same `{tier, name}` shape the Slint frontend writes,
        // so a cap set in either frontend applies in both.
        crate::settings_qt::save_pref("cast_quality_caps", serde_json::Value::Object(caps));

        let index = index.clamp(0, 3);
        cast_bridge::ui(move |mut b| {
            b.as_mut().set_device_cap_index(index);
        });
    }
}

// ---------------------------------------------------------------------------
// Entry points for the bridge (each hops onto the tokio runtime)
// ---------------------------------------------------------------------------

/// Whether the cast picker currently owns discovery.
///
/// Rust-side mirror of the `picker_open` qproperty, kept here because the
/// Diagnostics panel's on-demand scan has to decide whether stopping discovery
/// would kill a LIVE picker's device list — the reference reads
/// `CastState.picker-open` on the UI thread for exactly that gate
/// (`crates/qbz/src/diagnostics.rs:284-291`), and a qproperty cannot be read
/// off the Qt thread.
static PICKER_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// See [`PICKER_OPEN`].
pub(crate) fn picker_open() -> bool {
    PICKER_OPEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// Picker opened: arm discovery. This is the ONLY trigger — nothing scans in
/// the background and nothing touches the network before this runs.
pub(crate) fn open_picker() {
    PICKER_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
    set_error(String::new());
    let svc = service();
    crate::spawn(async move {
        svc.start_discovery().await;
    });
}

/// Picker closed: stop both discoveries. An active cast is untouched.
pub(crate) fn close_picker() {
    PICKER_OPEN.store(false, std::sync::atomic::Ordering::SeqCst);
    let svc = service();
    crate::spawn(async move {
        svc.stop_discovery().await;
    });
}

/// "Search again": re-arm the scan window + the refresh loop.
pub(crate) fn refresh() {
    let svc = service();
    crate::spawn(async move {
        svc.start_discovery().await;
    });
}

pub(crate) fn connect(device_id: String, protocol: String) {
    let svc = service();
    crate::spawn(async move {
        if let Err(e) = svc.connect(device_id, protocol).await {
            log::warn!("[qbz-qt][Cast] connect failed: {e}");
            set_error(e);
        }
    });
}

pub(crate) fn disconnect() {
    let svc = service();
    crate::spawn(async move {
        svc.disconnect().await;
    });
}

pub(crate) fn set_device_cap(cap_key: String, index: i32) {
    let svc = service();
    crate::spawn(async move {
        svc.set_device_cap(cap_key, index).await;
    });
}

/// Seed/refresh the cap dropdown labels + the live global-quality label.
/// Called from the bridge's `boot` and re-callable on a language or
/// streaming-quality change (option 0 embeds the live global label, and
/// Rust-pushed option models never re-translate on their own).
pub(crate) fn push_cap_options() {
    let key = crate::settings_qt::streaming_quality();
    let idx = crate::settings_qt::STREAMING_QUALITY_KEYS
        .iter()
        .position(|k| *k == key)
        .unwrap_or(3);
    // Tier names ("Hi-Res+", "CD Quality") are product names — untranslated
    // data, the same convention as the Settings streaming dropdown.
    let global_label = crate::settings_qt::STREAMING_QUALITY_LABELS[idx];
    let label_for = |k: &str| {
        crate::settings_qt::STREAMING_QUALITY_KEYS
            .iter()
            .position(|x| *x == k)
            .map(|i| crate::settings_qt::STREAMING_QUALITY_LABELS[i])
            .unwrap_or("")
    };
    // Index order matches `set_device_cap`: 0 follow · 1 hires · 2 cd · 3 mp3.
    let options = serde_json::json!([
        qbz_i18n::t_args("Follow app setting ({})", &[global_label]),
        label_for("hires"),
        label_for("cd"),
        label_for("mp3"),
    ]);
    let json = options.to_string();
    let global = global_label.to_string();
    cast_bridge::ui(move |mut b| {
        b.as_mut()
            .set_device_cap_options(cxx_qt_lib::QString::from(json.as_str()));
        b.as_mut()
            .set_global_quality_label(cxx_qt_lib::QString::from(global.as_str()));
    });
}

// ---------------------------------------------------------------------------
// Playback-path gates (call sites are GLUE in playback_qt.rs)
// ---------------------------------------------------------------------------

/// True while a renderer owns transport.
#[allow(dead_code)] // wired by playback_qt / the poll pump — see GLUE
pub(crate) async fn is_casting() -> bool {
    service().is_casting().await
}

/// Route the queue's CURRENT track to the connected renderer instead of
/// opening a local stream. Ok(false) = not casting, take the local path.
#[allow(dead_code)] // wired by playback_qt::play_queue_track — see GLUE
pub(crate) async fn play_current_if_cast(runtime: &Runtime) -> bool {
    let svc = service();
    if !svc.is_casting().await {
        return false;
    }
    match runtime.core().current_track().await {
        Some(track) => {
            if let Err(e) = svc.cast_track(&track).await {
                log::warn!("[qbz-qt][Cast] play new track {} failed: {e}", track.id);
                set_error(e);
            }
            true
        }
        // Connected but the queue has no current track: still handled — the
        // local path must not open a stream behind the renderer's back.
        None => true,
    }
}

// ---------------------------------------------------------------------------
// UI publishing helpers
// ---------------------------------------------------------------------------

fn set_scanning(scanning: bool) {
    cast_bridge::ui(move |mut b| b.as_mut().set_scanning(scanning));
}

fn set_error(message: String) {
    cast_bridge::ui(move |mut b| {
        b.as_mut()
            .set_error(cxx_qt_lib::QString::from(message.as_str()))
    });
}

fn push_quality(label: String, detail: String) {
    cast_bridge::ui(move |mut b| {
        b.as_mut()
            .set_quality_label(cxx_qt_lib::QString::from(label.as_str()));
        b.as_mut()
            .set_quality_detail(cxx_qt_lib::QString::from(detail.as_str()));
    });
}

fn push_position(position: f32, duration: f32, playing: bool) {
    cast_bridge::ui(move |mut b| {
        b.as_mut().set_position_secs(position);
        b.as_mut().set_duration_secs(duration);
        b.as_mut().set_is_playing(playing);
    });
}

/// Serialize both device lists into the ONE document the picker reads
/// (the home_bridge JSON-document convention: cxx-qt-lib 0.7 cannot express
/// a QVariantList of QVariantMaps).
fn push_devices(chromecast: Vec<DiscoveredDevice>, dlna: Vec<DiscoveredDlnaDevice>) {
    let cc_count = chromecast.len() as i32;
    let dl_count = dlna.len() as i32;
    let mut rows: Vec<serde_json::Value> = Vec::with_capacity(chromecast.len() + dlna.len());
    for d in chromecast {
        rows.push(serde_json::json!({
            "id": d.id,
            "name": d.name,
            "ip": d.ip,
            "protocol": "chromecast",
            "model": d.model,
            "canPlay": true,
            "canSetVolume": true,
        }));
    }
    for d in dlna {
        rows.push(serde_json::json!({
            "id": d.id,
            "name": d.name,
            "ip": d.ip,
            "protocol": "dlna",
            "model": d.model,
            "canPlay": d.has_av_transport,
            "canSetVolume": d.has_rendering_control,
        }));
    }
    let json = serde_json::Value::Array(rows).to_string();
    cast_bridge::ui(move |mut b| {
        b.as_mut()
            .set_devices_json(cxx_qt_lib::QString::from(json.as_str()));
        b.as_mut().set_chromecast_count(cc_count);
        b.as_mut().set_dlna_count(dl_count);
    });
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// The tier to REQUEST for a cast to the renderer keyed `cap_key`, plus the
/// request-time cause: the user's streaming preference clamped by the manual
/// per-renderer cap, lowest tier wins (#638 fix 4). On a tie the cause is
/// `RendererCap` — the more specific, more surprising of the two. NEVER
/// consults a local DAC cap: the local DAC is not in a cast's signal path.
fn effective_cast_quality(cap_key: Option<&str>) -> (Quality, QualityLimit) {
    let pref = crate::playback_qt::current_quality();
    let cap = cap_key.and_then(|k| {
        cast_caps()
            .get(k)
            .and_then(|c| c.get("tier"))
            .and_then(|t| t.as_str())
            .map(quality_for_tier)
    });
    match cap {
        // A stored `hires_plus`/unknown tier resolves to UltraHiRes = no
        // effective cap (and never a RendererCap cause).
        Some(cap) if cap < pref => (Quality::min_tier(cap, pref), QualityLimit::RendererCap),
        Some(cap) if cap == pref && cap < Quality::UltraHiRes => (pref, QualityLimit::RendererCap),
        _ => (
            pref,
            if pref < Quality::UltraHiRes {
                QualityLimit::Preference
            } else {
                QualityLimit::None
            },
        ),
    }
}

/// Cap tier key -> request tier (the Slint `streaming_quality_for_key`).
fn quality_for_tier(tier: &str) -> Quality {
    match tier {
        "mp3" => Quality::Mp3,
        "cd" => Quality::Lossless,
        "hires" => Quality::HiRes,
        _ => Quality::UltraHiRes,
    }
}

/// The `cast_quality_caps` map out of the SHARED ui_prefs.json. Read raw
/// because `settings_qt`'s typed accessors only cover scalars and its
/// `prefs_path` is private; writes still go through `settings_qt::save_pref`,
/// which is the additive patch that preserves every other frontend's keys.
fn cast_caps() -> serde_json::Map<String, serde_json::Value> {
    let Some(path) = dirs::data_dir().map(|p| p.join("qbz").join("ui_prefs.json")) else {
        return serde_json::Map::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v.get("cast_quality_caps").cloned())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Dropdown index for the stored cap of `cap_key` (0 follow · 1 hires · 2 cd ·
/// 3 mp3). An unknown stored tier reads as "follow", so a hand-edited value
/// degrades to the no-cap default instead of showing a wrong cap.
fn cap_index_for_key(cap_key: &str) -> i32 {
    match cast_caps()
        .get(cap_key)
        .and_then(|c| c.get("tier"))
        .and_then(|t| t.as_str())
    {
        Some("hires") => 1,
        Some("cd") => 2,
        Some("mp3") => 3,
        _ => 0,
    }
}

/// The on-disk file a row can be SERVED from, or a named refusal.
///
/// IC-10. This was `resolve_local_path(track_id)`: an ephemeral-floor test and
/// then a bare `db.get_track(id)`. Its two siblings in `local_playback` both
/// grew guards this one never got — and the id it is handed is a
/// `QueueTrack.id`, which is only sometimes a `library.db` rowid. A Plex row
/// carries a NAMESPACED id (bit 40 set) and an offline row carries the *Qobuz*
/// catalog id, so both looked up a rowid that is not theirs, missed, and
/// reported "Local file not found" — an answer that names the wrong problem.
///
/// It is now a `PlaybackTicket` consumer like every other playback path, which
/// makes the refusals structural rather than remembered:
///
/// - `File` / `DsdFile` — a real path; cast can serve it.
/// - anything else — the row plays through the network, and casting it would
///   need a proxy this service does not have. That is the `TODO(cast-plex)`
///   arm's actual content, and it now applies to every remote source by
///   construction instead of to the one word somebody thought to list.
async fn resolve_castable_path(track: &QueueTrack) -> Result<String, String> {
    let ticket = qbz_source::registry()
        .playback(track)
        .await
        .map_err(|e| format!("Cannot resolve track {} for casting: {e}", track.id))?;
    match ticket {
        qbz_source::PlaybackTicket::File { path, .. }
        | qbz_source::PlaybackTicket::DsdFile { path, .. } => {
            Ok(path.to_string_lossy().into_owned())
        }
        other => Err(format!(
            "Track {} cannot be cast: its source serves it over the network, not as a file ({})",
            track.id,
            match other {
                qbz_source::PlaybackTicket::Stream { .. } => "streamed",
                qbz_source::PlaybackTicket::Bytes { .. } => "fetched bytes",
                qbz_source::PlaybackTicket::Catalog { .. } => "catalog",
                _ => "unsupported",
            }
        )),
    }
}

/// Content type for a local file by extension (for the UI label; the SERVED
/// MIME is set by the crate's own map in `register_file`).
fn content_type_for_local(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "m4a" | "alac" | "mp4" => "audio/mp4",
        "aiff" | "aif" => "audio/aiff",
        // DSD containers (MinimServer DLNA convention).
        "dsf" => "audio/x-dsf",
        "dff" => "audio/x-dff",
        "ape" => "audio/x-ape",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "aac" => "audio/aac",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn media_metadata(track: &QueueTrack) -> MediaMetadata {
    MediaMetadata {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        artwork_url: track.artwork_url.clone(),
        duration_secs: Some(track.duration_secs),
    }
}

fn dlna_metadata(track: &QueueTrack) -> DlnaMetadata {
    DlnaMetadata {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        artwork_url: track.artwork_url.clone(),
        duration_secs: Some(track.duration_secs),
    }
}

/// FALLBACK quality label + detail from the track's CATALOG metadata — used
/// only when the STREAMINFO probe cannot read the served bytes (non-FLAC /
/// local files). Returns (label, detail), e.g. ("Hi-Res FLAC", "24-bit / 96 kHz").
fn quality_label_from_track(track: &QueueTrack) -> (String, String) {
    // DSD carries bit_depth=1 + the DSD bit rate as sample_rate — the generic
    // format would print the nonsense "1-bit / 2822.4 kHz".
    if track.bit_depth == Some(1) {
        let dsd = crate::quality_state::dsd_multiple_label(track.sample_rate);
        return (dsd.clone(), dsd);
    }
    let detail = match (track.sample_rate, track.bit_depth) {
        (Some(khz), Some(bits)) => {
            format!("{}-bit / {} kHz", bits, crate::home_qt::format_rate(khz))
        }
        (Some(khz), None) => format!("{} kHz", crate::home_qt::format_rate(khz)),
        (None, Some(bits)) => format!("{bits}-bit"),
        (None, None) => String::new(),
    };
    let label = if track.hires { "Hi-Res FLAC" } else { "FLAC" }.to_string();
    (label, detail)
}
