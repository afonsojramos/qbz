//! Headless QConnect integration.
//!
//! Connects to Qobuz's WebSocket servers so the daemon can be
//! controlled from the Qobuz mobile app or web player.
//!
//! Uses the qconnect-app crate directly (Tauri-free).

use std::sync::Arc;
use async_trait::async_trait;
use qconnect_app::{QconnectApp, QconnectAppEvent, QconnectEventSink};
use qconnect_transport_ws::{NativeWsTransport, WsTransportConfig};
use tokio::sync::broadcast;

use crate::adapter::{DaemonAdapter, DaemonEvent};

/// Event sink that routes QConnect events to the daemon event bus.
pub struct HeadlessQconnectSink {
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl HeadlessQconnectSink {
    pub fn new(event_tx: broadcast::Sender<DaemonEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl QconnectEventSink for HeadlessQconnectSink {
    async fn on_event(&self, event: QconnectAppEvent) {
        match &event {
            QconnectAppEvent::TransportConnected => {
                log::info!("[qbzd/qconnect] Connected to Qobuz servers");
            }
            QconnectAppEvent::TransportDisconnected => {
                log::info!("[qbzd/qconnect] Disconnected from Qobuz servers");
            }
            QconnectAppEvent::RendererCommandApplied { command, .. } => {
                log::debug!("[qbzd/qconnect] Renderer command: {:?}", command);
            }
            _ => {}
        }
        // Future: map QConnect events to DaemonEvents for SSE clients
    }
}

/// Start QConnect if enabled and user is logged in.
/// Returns the QconnectApp handle for later control.
pub async fn start_qconnect(
    core: &qbz_core::QbzCore<DaemonAdapter>,
    event_tx: broadcast::Sender<DaemonEvent>,
    device_name: &str,
) -> Option<Arc<QconnectApp<NativeWsTransport, HeadlessQconnectSink>>> {
    // Get credentials from the Qobuz client
    let client_arc = core.client();
    let client_guard = client_arc.read().await;
    let client = client_guard.as_ref()?;

    let app_id = client.app_id().await.ok()?;
    let auth_token = client.auth_token().await.ok()?;

    // Get QWS endpoint and JWT token
    let (endpoint_url, jwt_qws) = fetch_qws_credentials(client).await?;

    let transport = Arc::new(NativeWsTransport::new());
    let sink = Arc::new(HeadlessQconnectSink::new(event_tx));
    let app = Arc::new(QconnectApp::new(transport.clone(), sink));

    let config = WsTransportConfig {
        endpoint_url,
        jwt_qws: Some(jwt_qws),
        ..Default::default()
    };

    match app.connect(config).await {
        Ok(()) => {
            log::info!("[qbzd/qconnect] QConnect started as '{}'", device_name);
            Some(app)
        }
        Err(e) => {
            log::warn!("[qbzd/qconnect] Failed to connect: {}", e);
            None
        }
    }
}

/// Fetch QWS WebSocket credentials from Qobuz API.
async fn fetch_qws_credentials(
    client: &qbz_qobuz::QobuzClient,
) -> Option<(String, String)> {
    let app_id = client.app_id().await.ok()?;
    let auth_token = client.auth_token().await.ok()?;

    let http = reqwest::Client::new();
    let resp = http
        .post("https://www.qobuz.com/api.json/0.2/qws/createToken")
        .header("X-App-Id", &app_id)
        .header("X-User-Auth-Token", &auth_token)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        log::warn!("[qbzd/qconnect] qws/createToken failed: {}", resp.status());
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let endpoint = body.get("endpoint_url")?.as_str()?.to_string();
    let jwt = body
        .get("tokens")
        .and_then(|t| t.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|t| {
                if t.get("kind")?.as_str()? == "jwt_qws" {
                    Some(t.get("token")?.as_str()?.to_string())
                } else {
                    None
                }
            })
        })?;

    Some((endpoint, jwt))
}
