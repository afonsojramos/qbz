//! Qobuz Connect renderer engine for the Qt frontend (block B2 of the
//! 2026-08-01 QConnect Qt-port contract).
//!
//! Behavior-1:1 port of the Slint `qbz/src/qconnect_engine.rs`. Implements
//! [`qconnect_app::QconnectRendererEngine`] over the Qt `AppRuntime`'s `QbzCore`
//! + `Player`, so qbz-qt becomes a QConnect renderer that inherits the shared
//! echo/cursor/materialize/shuffle orchestration in `qconnect_app::renderer`
//! instead of re-deriving it.
//!
//! The protected bit-perfect seams (`play_streaming_dynamic` / `play_data`) and
//! the HTTP feeder live here, impl-side, exactly as the Tauri `CoreBridge` impl
//! does; the probe-derived sample_rate/channels/bit_depth flow STRAIGHT into
//! `play_streaming_dynamic` (never defaulted, or hi-res remote playback silently
//! resamples). The feeder body is a near-verbatim port of the Tauri
//! `track_loading.rs` feeder, with `bridge.player()` -> `self.core().player()`;
//! the only deviation is the TLS backend — the crates workspace `reqwest` ships
//! `rustls-tls` (not `native-tls`), so the `.use_native_tls()` calls are dropped.
//! TLS is transport encryption only; the decoded audio bytes are identical, so
//! bit-perfect is unaffected. (If the Qobuz streaming CDN ever presents a cert
//! rustls rejects, add `native-tls` to the frontend crate's reqwest features.)
//!
//! The feeder helpers (HEAD+range FLAC probe, chunked GET -> `BufferWriter`,
//! Akamai >100-header detection) are PRIVATE copies of the Slint crate's
//! `remote_stream.rs`, folded into this file per contract §1.3 — deliberately
//! NOT a shared module: the Slint crate keeps its own copy too, and extracting
//! commonalities into a shared crate is out of scope (backend untouchable).
//!
//! Wired by the QConnect facade (block B3) + event sink (block B4); until
//! those land the constructor is unused, so the module keeps the reference's
//! `#![allow(dead_code)]` (same convention as `toast_qt.rs`).
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use qbz_app::shell::AppRuntime;
use qbz_core::{LoggingAdapter, QbzCore};
use qbz_models::{Quality, QueueTrack, RepeatMode, Track};
use qbz_player::{BufferWriter, PlaybackState, Player};
use qconnect_app::QconnectRendererEngine;

/// QConnect renderer engine backed by the Qt `AppRuntime`. Holds the shared
/// runtime and forwards every trait method through `runtime.core()`; the async
/// feeder spawns on the ambient tokio runtime (`start_track_stream` is always
/// awaited from a runtime task).
pub struct QtRendererEngine {
    runtime: Arc<AppRuntime<LoggingAdapter>>,
}

impl QtRendererEngine {
    pub fn new(runtime: Arc<AppRuntime<LoggingAdapter>>) -> Self {
        Self { runtime }
    }

    fn core(&self) -> &Arc<QbzCore<LoggingAdapter>> {
        self.runtime.core()
    }

    /// Last-resort load for tracks the raw-URL path cannot fetch (the CDN
    /// header flood defeats every reqwest attempt — see
    /// `is_header_flood_error`): the CMAF path is unaffected by the h1 header
    /// cap. `play_track_resolved` does NOT move the queue cursor (nothing on
    /// the QConnect path does — the shared driver's cursor sync only fires on a
    /// playing->playing track edge), so sync it explicitly or the now-playing
    /// truth keeps showing the PREVIOUS track while the recovered one plays.
    async fn play_via_cmaf(
        &self,
        track_id: u64,
        quality: Quality,
        start_position_secs: u64,
    ) -> Result<(), String> {
        self.core()
            .play_track_resolved(track_id, quality, None, None, start_position_secs)
            .await
            .map_err(|err| format!("CMAF fallback for remote track {track_id}: {err}"))?;
        self.core().sync_current_to_id(track_id).await;
        Ok(())
    }
}

#[async_trait]
impl QconnectRendererEngine for QtRendererEngine {
    // ---- transport (sync) ----
    fn resume(&self) -> Result<(), String> {
        self.core().resume().map_err(|err| err.to_string())
    }
    fn pause(&self) -> Result<(), String> {
        self.core().pause().map_err(|err| err.to_string())
    }
    fn stop(&self) -> Result<(), String> {
        self.core().stop().map_err(|err| err.to_string())
    }
    fn seek(&self, position_secs: u64) -> Result<(), String> {
        self.core().seek(position_secs).map_err(|err| err.to_string())
    }
    fn set_volume(&self, fraction: f32) -> Result<(), String> {
        self.core().set_volume(fraction).map_err(|err| err.to_string())
    }
    fn get_playback_state(&self) -> PlaybackState {
        self.core().get_playback_state()
    }
    fn has_loaded_audio(&self) -> bool {
        self.core().player().has_loaded_audio()
    }

    // ---- queue / mode (async) ----
    async fn set_repeat_mode(&self, mode: RepeatMode) {
        self.core().set_repeat_mode(mode).await
    }
    async fn set_shuffle(&self, enabled: bool) {
        self.core().set_shuffle(enabled).await
    }
    async fn set_shuffle_flag(&self, enabled: bool) {
        self.core().set_shuffle_with_order(enabled, None).await
    }
    async fn get_all_queue_tracks(&self) -> (Vec<QueueTrack>, Option<usize>) {
        self.core().get_all_queue_tracks().await
    }
    async fn set_queue(&self, tracks: Vec<QueueTrack>, start_index: Option<usize>) {
        self.core().set_queue(tracks, start_index).await
    }
    async fn set_queue_with_order(
        &self,
        tracks: Vec<QueueTrack>,
        start_index: Option<usize>,
        shuffle_enabled: bool,
        shuffle_order: Option<Vec<usize>>,
    ) {
        self.core()
            .set_queue_with_order(tracks, start_index, shuffle_enabled, shuffle_order)
            .await
    }
    async fn clear_queue(&self, keep_current: bool) {
        self.core().clear_queue(keep_current).await
    }
    async fn play_index(&self, index: usize) -> Option<QueueTrack> {
        self.core().play_index(index).await
    }

    // ---- catalog (async) ----
    async fn get_track(&self, track_id: u64) -> Result<Track, String> {
        self.core()
            .get_track(track_id)
            .await
            .map_err(|err| err.to_string())
    }
    async fn get_tracks_batch(&self, track_ids: &[u64]) -> Result<Vec<Track>, String> {
        self.core()
            .get_tracks_batch(track_ids)
            .await
            .map_err(|err| err.to_string())
    }

    // ---- protected audio seam (the only protected touch) ----
    async fn start_track_stream(
        &self,
        track_id: u64,
        quality: Quality,
        duration_secs: u64,
        start_position_secs: u64,
    ) -> Result<(), String> {
        let stream_url = self
            .core()
            .get_stream_url(track_id, quality)
            .await
            .map_err(|err| format!("resolve stream url for remote track {track_id}: {err}"))?;

        let player = self.core().player();
        let stream_result = stream_remote_track_into_player(
            &player,
            track_id,
            duration_secs,
            start_position_secs,
            &stream_url.url,
            "QConnect",
        )
        .await;

        let Err(stream_err) = stream_result else {
            return Ok(());
        };

        // Akamai small-object header flood: SMALL raw-url objects come back
        // with ~106 headers, over hyper's hard-coded 100-header h1 cap, so
        // EVERY reqwest fetch of this URL fails — the full download would die
        // the same death. Skip it and go straight to the CMAF last resort.
        if is_header_flood_error(&stream_err) {
            log::warn!(
                "[QConnect] Raw-URL streaming hit the CDN header flood for track {track_id}: {stream_err}. Skipping full download; last resort: CMAF."
            );
            return self
                .play_via_cmaf(track_id, quality, start_position_secs)
                .await;
        }

        log::warn!(
            "[QConnect] Streaming handoff unavailable for track {}: {}. Falling back to full download.",
            track_id,
            stream_err
        );
        match download_remote_audio(&stream_url.url).await {
            Ok(audio_data) => {
                self.core()
                    .player()
                    .play_data(audio_data, track_id)
                    .map_err(|err| format!("play remote track {track_id}: {err}"))?;
                Ok(())
            }
            Err(download_err) if is_header_flood_error(&download_err) => {
                log::warn!(
                    "[QConnect] Full download hit the CDN header flood for track {track_id}: {download_err}. Last resort: CMAF."
                );
                self.play_via_cmaf(track_id, quality, start_position_secs)
                    .await
            }
            Err(download_err) => Err(download_err),
        }
    }

    fn current_output_format(&self) -> Option<(u32, u32)> {
        let player = self.core().player();
        Some((
            player.state.get_sample_rate(),
            player.state.get_bit_depth(),
        ))
    }
}


async fn download_remote_audio(url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|err| {
            format!(
                "download remote audio request failed: {}",
                describe_reqwest_error(&err)
            )
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "download remote audio failed with status {}",
            response.status()
        ));
    }

    let bytes = response.bytes().await.map_err(|err| {
        format!(
            "read remote audio bytes failed: {}",
            describe_reqwest_error(&err)
        )
    })?;
    Ok(bytes.to_vec())
}

// ---------------------------------------------------------------------------
// HTTP feeder helpers — private copies of the Slint crate's `remote_stream.rs`
// (contract §1.3; the Slint `qconnect_engine.rs` reaches the same bodies via
// `crate::remote_stream::`). Progressive feeder: probe a remote audio URL for
// size + FLAC format, open the player's progressive streaming sink
// (`Player::play_streaming_dynamic`), then push the body to the returned
// `BufferWriter` chunk-by-chunk as it arrives. Playback starts as soon as the
// initial buffer fills — not after the whole file lands.
//
// BIT-PERFECT: `play_streaming_dynamic` decodes the same original bytes and
// drives the PROTECTED device init from the decoded stream. The
// `sample_rate`/`bit_depth` parsed here are only the streaming-config hints;
// the audio backend (`pipewire_backend.rs`, `init_device`, `audio_settings.rs`)
// is untouched.
// ---------------------------------------------------------------------------

/// Format/size facts sniffed from a remote audio URL before streaming.
struct RemoteStreamInfo {
    content_length: u64,
    sample_rate: u32,
    channels: u16,
    bit_depth: u32,
    speed_mbps: f64,
}

/// Probe + open the progressive sink + spawn the background feeder.
///
/// On success the player has begun buffering and `play_streaming_dynamic` will
/// start audio once the initial buffer fills; the body download runs in a
/// spawned task. Errors here mean the caller should fall back to a full
/// download (the probe or the sink open failed).
async fn stream_remote_track_into_player(
    player: &Player,
    track_id: u64,
    duration_secs: u64,
    start_position_secs: u64,
    url: &str,
    log_tag: &str,
) -> Result<(), String> {
    let stream_info = probe_remote_stream_info(url).await?;
    log::info!(
        "[{}/STREAMING] Track {} - {:.2} MB, {}Hz, {} ch, {}-bit, {:.1} MB/s",
        log_tag,
        track_id,
        stream_info.content_length as f64 / (1024.0 * 1024.0),
        stream_info.sample_rate,
        stream_info.channels,
        stream_info.bit_depth,
        stream_info.speed_mbps
    );

    let writer = player
        .play_streaming_dynamic(
            track_id,
            stream_info.sample_rate,
            stream_info.channels,
            stream_info.bit_depth,
            stream_info.content_length,
            stream_info.speed_mbps,
            duration_secs,
            start_position_secs,
        )
        .map_err(|err| format!("start streaming remote track {track_id}: {err}"))?;

    let url = url.to_string();
    let content_length = stream_info.content_length;
    let log_tag = log_tag.to_string();
    tokio::spawn(async move {
        if let Err(err) =
            download_and_stream_remote_track(&url, writer, track_id, content_length, &log_tag).await
        {
            log::error!(
                "[{}/STREAMING] Track {} failed while streaming: {}",
                log_tag,
                track_id,
                err
            );
        }
    });

    Ok(())
}

/// HEAD for content-length, then a small `Range: bytes=0-65535` GET to (a)
/// measure throughput and (b) parse the FLAC `STREAMINFO` block for the real
/// sample rate / channels / bit depth. Never defaults silently for FLAC (a
/// wrong sample rate would silently resample hi-res).
async fn probe_remote_stream_info(url: &str) -> Result<RemoteStreamInfo, String> {
    use std::time::Instant;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| format!("create stream probe client: {err}"))?;

    let head_response = client
        .head(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|err| format!("probe HEAD request failed: {}", describe_reqwest_error(&err)))?;

    if !head_response.status().is_success() {
        return Err(format!(
            "probe HEAD request failed with status {}",
            head_response.status()
        ));
    }

    let content_length = head_response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| "probe missing content-length header".to_string())?;

    let start_time = Instant::now();
    let range_response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .header("Range", "bytes=0-65535")
        .send()
        .await
        .map_err(|err| format!("probe range request failed: {}", describe_reqwest_error(&err)))?;

    if !range_response.status().is_success() {
        return Err(format!(
            "probe range request failed with status {}",
            range_response.status()
        ));
    }

    let initial_bytes = range_response
        .bytes()
        .await
        .map_err(|err| format!("read probe bytes failed: {}", describe_reqwest_error(&err)))?;

    let elapsed = start_time.elapsed();
    let speed_mbps = if elapsed.as_secs_f64() > 0.0 {
        (initial_bytes.len() as f64 / elapsed.as_secs_f64()) / (1024.0 * 1024.0)
    } else {
        10.0
    };

    // STREAMINFO parse via the shared prober (hoisted to qbz-models for the
    // cast path, #638 fix 1). The prober never guesses — the CD-shaped
    // defaults for a non-FLAC probe stay HERE so this path's behavior is
    // byte-identical to the original inline parse.
    let (sample_rate, channels, bit_depth) = match qbz_models::probe_streaminfo(&initial_bytes) {
        Some(p) => (p.sample_rate, p.channels, p.bits_per_sample),
        None => {
            log::warn!("[remote-stream] Non-FLAC probe for remote handoff, using defaults");
            (44_100, 2, 16)
        }
    };

    Ok(RemoteStreamInfo {
        content_length,
        sample_rate,
        channels,
        bit_depth,
        speed_mbps,
    })
}

/// Plain full-body GET → `bytes_stream()` loop → `writer.push_chunk` →
/// `writer.complete()`. No HTTP Range on the main GET (the `BufferedMediaSource`
/// buffers every pushed byte and serves seeks from the growing buffer).
async fn download_and_stream_remote_track(
    url: &str,
    writer: BufferWriter,
    track_id: u64,
    content_length: u64,
    log_tag: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::time::Instant;

    struct FailGuard {
        writer: BufferWriter,
        armed: bool,
    }
    impl Drop for FailGuard {
        fn drop(&mut self) {
            if self.armed {
                let _ = self
                    .writer
                    .error("remote stream aborted before completion".into());
            }
        }
    }
    let mut guard = FailGuard {
        writer,
        armed: true,
    };
    let writer = &guard.writer;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|err| format!("create remote streaming client: {err}"))?;

    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|err| {
            format!(
                "start remote streaming request failed: {}",
                describe_reqwest_error(&err)
            )
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "remote streaming request failed with status {}",
            response.status()
        ));
    }

    let mut bytes_received = 0u64;
    let mut stream = response.bytes_stream();
    let start_time = Instant::now();
    let mut last_log_time = Instant::now();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result
            .map_err(|err| format!("remote streaming chunk failed: {}", describe_reqwest_error(&err)))?;
        bytes_received += chunk.len() as u64;

        if let Err(err) = writer.push_chunk(&chunk) {
            log::error!(
                "[{}/STREAMING] Failed to push chunk for track {}: {}",
                log_tag,
                track_id,
                err
            );
            guard.armed = false;
            let _ = writer.error(format!("push_chunk failed: {err}"));
            return Err(format!("push_chunk failed: {err}"));
        }

        let now = Instant::now();
        if now.duration_since(last_log_time) >= Duration::from_secs(2) && content_length > 0 {
            let progress = (bytes_received as f64 / content_length as f64) * 100.0;
            let avg_speed =
                (bytes_received as f64 / start_time.elapsed().as_secs_f64()) / (1024.0 * 1024.0);
            log::info!(
                "[{}/STREAMING] Track {} {:.1}% ({:.2}/{:.2} MB) @ {:.2} MB/s",
                log_tag,
                track_id,
                progress,
                bytes_received as f64 / (1024.0 * 1024.0),
                content_length as f64 / (1024.0 * 1024.0),
                avg_speed
            );
            last_log_time = now;
        }
    }

    guard.armed = false;
    if let Err(err) = writer.complete() {
        log::error!(
            "[{}/STREAMING] Failed to mark stream complete for track {}: {}",
            log_tag,
            track_id,
            err
        );
        let _ = writer.error(format!("complete failed: {err}"));
        return Err(format!("complete failed: {err}"));
    }

    log::info!(
        "[{}/STREAMING] Track {} complete: {:.2} MB in {:.1}s",
        log_tag,
        track_id,
        bytes_received as f64 / (1024.0 * 1024.0),
        start_time.elapsed().as_secs_f64()
    );

    Ok(())
}

/// reqwest's `Display` hides the source chain — which is exactly where the
/// diagnosis lives (Akamai's >100-header small-object flood surfaces as hyper's
/// "message head is too large" two levels down). Walk `source()` and join the
/// chain so logs AND signature matching see the real cause.
fn describe_reqwest_error(err: &reqwest::Error) -> String {
    use std::error::Error as _;
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// True when an error message (already chain-expanded by
/// [`describe_reqwest_error`]) shows hyper's hard-coded h1 100-header cap.
/// Akamai answers SMALL raw-url objects with ~106 headers (the `X-AK-GRN` /
/// `X-AK-FWD-ERROR: ERR_POC_FWD_OBJ_TOO_SMALL` flood), so EVERY reqwest fetch
/// of such an URL fails this way — streaming probe and full download alike.
fn is_header_flood_error(message: &str) -> bool {
    let haystack = message.to_ascii_lowercase();
    haystack.contains("message head is too large") || haystack.contains("too many headers")
}
