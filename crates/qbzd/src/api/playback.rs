use std::sync::Arc;
use axum::Json;
use serde::Deserialize;

use crate::daemon::DaemonCore;

#[derive(Deserialize)]
pub struct SeekRequest {
    pub position_secs: u64,
}

#[derive(Deserialize)]
pub struct VolumeRequest {
    pub volume: f32,
}

pub async fn get_playback(daemon: Arc<DaemonCore>) -> Json<serde_json::Value> {
    let player = daemon.core.player();
    let state = &player.state;
    Json(serde_json::json!({
        "state": if state.is_playing() { "Playing" } else if state.current_track_id() != 0 { "Paused" } else { "Stopped" },
        "track_id": state.current_track_id(),
        "position_secs": state.current_position(),
        "duration_secs": state.duration(),
        "volume": state.volume(),
        "sample_rate": state.get_sample_rate(),
        "bit_depth": state.get_bit_depth(),
    }))
}

#[derive(Deserialize)]
pub struct PlayTrackRequest {
    pub track_id: u64,
    pub quality: Option<String>,
}

/// Play a specific track by ID. Downloads audio from Qobuz and feeds to player.
pub async fn play_track(
    daemon: Arc<DaemonCore>,
    Json(req): Json<PlayTrackRequest>,
) -> Result<Json<serde_json::Value>, String> {
    let quality = match req.quality.as_deref() {
        Some("Hi-Res+") | Some("UltraHiRes") => qbz_models::Quality::UltraHiRes,
        Some("Hi-Res") | Some("HiRes") => qbz_models::Quality::HiRes,
        Some("Lossless") => qbz_models::Quality::Lossless,
        _ => qbz_models::Quality::HiRes, // Default to HiRes
    };

    log::info!("[qbzd/play] Playing track {} (quality: {:?})", req.track_id, quality);

    // Get stream URL from Qobuz
    let stream_url = daemon.core.get_stream_url(req.track_id, quality)
        .await
        .map_err(|e| format!("Failed to get stream URL: {}", e))?;

    log::info!("[qbzd/play] Stream: {}Hz, {:?}bit",
        (stream_url.sampling_rate * 1000.0) as u32,
        stream_url.bit_depth,
    );

    // Download the audio
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = http
        .get(&stream_url.url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download HTTP {}", response.status()));
    }

    let audio_data = response
        .bytes()
        .await
        .map_err(|e| format!("Read failed: {}", e))?
        .to_vec();

    log::info!("[qbzd/play] Downloaded {} bytes, feeding to player", audio_data.len());

    // Feed to player
    let player = daemon.core.player();
    player
        .play_data(audio_data, req.track_id)
        .map_err(|e| format!("Player error: {}", e))?;

    // Cache the audio
    daemon.audio_cache.insert(req.track_id, vec![]); // TODO: cache actual data

    Ok(Json(serde_json::json!({
        "playing": true,
        "track_id": req.track_id,
        "sample_rate": stream_url.sampling_rate,
        "bit_depth": stream_url.bit_depth,
    })))
}

pub async fn play(daemon: Arc<DaemonCore>) -> Result<&'static str, String> {
    daemon.core.resume().map_err(|e| e.to_string())?;
    Ok("ok")
}

pub async fn pause(daemon: Arc<DaemonCore>) -> Result<&'static str, String> {
    daemon.core.pause().map_err(|e| e.to_string())?;
    Ok("ok")
}

pub async fn stop(daemon: Arc<DaemonCore>) -> Result<&'static str, String> {
    daemon.core.stop().map_err(|e| e.to_string())?;
    Ok("ok")
}

pub async fn next(daemon: Arc<DaemonCore>) -> Json<serde_json::Value> {
    let track = daemon.core.next_track().await;
    Json(serde_json::json!({
        "track": track,
    }))
}

pub async fn previous(daemon: Arc<DaemonCore>) -> Json<serde_json::Value> {
    let track = daemon.core.previous_track().await;
    Json(serde_json::json!({
        "track": track,
    }))
}

pub async fn seek(
    daemon: Arc<DaemonCore>,
    Json(req): Json<SeekRequest>,
) -> Result<&'static str, String> {
    daemon.core.seek(req.position_secs).map_err(|e| e.to_string())?;
    Ok("ok")
}

pub async fn volume(
    daemon: Arc<DaemonCore>,
    Json(req): Json<VolumeRequest>,
) -> Result<&'static str, String> {
    daemon.core.set_volume(req.volume).map_err(|e| e.to_string())?;
    Ok("ok")
}
