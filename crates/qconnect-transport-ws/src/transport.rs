use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use prost::{
    encoding::{decode_varint, encode_varint},
    Message,
};
use qconnect_protocol::{
    decode_inbound_json, decode_queue_server_events, decode_renderer_server_commands,
    encode_outbound_payload_bytes, InboundEnvelope, OutboundEnvelope, QueueServerEvent,
    RendererServerCommand,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc, watch, Mutex},
    task::JoinHandle,
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message as WsMessage},
};

use crate::{WsTransportConfig, WsTransportError};

const MSG_TYPE_AUTHENTICATE: u8 = 1;
const MSG_TYPE_SUBSCRIBE: u8 = 2;
const MSG_TYPE_PAYLOAD: u8 = 6;
const MSG_TYPE_ERROR: u8 = 9;
const MSG_TYPE_DISCONNECT: u8 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportEvent {
    Connected,
    Disconnected,
    Authenticated,
    Subscribed,
    ReconnectScheduled {
        attempt: u32,
        backoff_ms: u64,
        reason: String,
    },
    KeepalivePingSent,
    KeepalivePongReceived,
    TransportError {
        stage: String,
        message: String,
    },
    InboundFrameDecoded {
        cloud_message_type: u8,
        payload_size: usize,
    },
    InboundPayloadBytes {
        cloud_message_type: u8,
        payload: Vec<u8>,
    },
    OutboundSent {
        message_type: String,
        action_uuid: String,
    },
    InboundQueueServerEvent(QueueServerEvent),
    InboundRendererServerCommand(RendererServerCommand),
    InboundReceived(InboundEnvelope),
}

#[async_trait]
pub trait WsTransport: Send + Sync {
    async fn connect(&self, config: WsTransportConfig) -> Result<(), WsTransportError>;
    async fn disconnect(&self) -> Result<(), WsTransportError>;
    async fn send(&self, envelope: OutboundEnvelope) -> Result<(), WsTransportError>;
    fn subscribe(&self) -> broadcast::Receiver<TransportEvent>;
}

#[derive(Debug, Default)]
struct InMemoryState {
    connected: bool,
    last_config: Option<WsTransportConfig>,
    sent_messages: Vec<OutboundEnvelope>,
}

#[derive(Clone)]
pub struct InMemoryWsTransport {
    state: Arc<Mutex<InMemoryState>>,
    events_tx: broadcast::Sender<TransportEvent>,
}

impl InMemoryWsTransport {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(512);
        Self {
            state: Arc::new(Mutex::new(InMemoryState::default())),
            events_tx,
        }
    }

    pub async fn inject_inbound(&self, envelope: InboundEnvelope) -> Result<(), WsTransportError> {
        self.events_tx
            .send(TransportEvent::InboundReceived(envelope))
            .map_err(|_| WsTransportError::EventChannelClosed)?;
        Ok(())
    }

    pub async fn sent_messages(&self) -> Vec<OutboundEnvelope> {
        self.state.lock().await.sent_messages.clone()
    }

    pub async fn is_connected(&self) -> bool {
        self.state.lock().await.connected
    }
}

impl Default for InMemoryWsTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WsTransport for InMemoryWsTransport {
    async fn connect(&self, config: WsTransportConfig) -> Result<(), WsTransportError> {
        let mut state = self.state.lock().await;
        if state.connected {
            return Err(WsTransportError::AlreadyConnected);
        }

        state.connected = true;
        state.last_config = Some(config);
        drop(state);

        self.events_tx
            .send(TransportEvent::Connected)
            .map_err(|_| WsTransportError::EventChannelClosed)?;
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), WsTransportError> {
        let mut state = self.state.lock().await;
        if !state.connected {
            return Err(WsTransportError::NotConnected);
        }

        state.connected = false;
        drop(state);

        self.events_tx
            .send(TransportEvent::Disconnected)
            .map_err(|_| WsTransportError::EventChannelClosed)?;
        Ok(())
    }

    async fn send(&self, envelope: OutboundEnvelope) -> Result<(), WsTransportError> {
        let mut state = self.state.lock().await;
        if !state.connected {
            return Err(WsTransportError::NotConnected);
        }

        state.sent_messages.push(envelope.clone());
        drop(state);

        self.events_tx
            .send(TransportEvent::OutboundSent {
                message_type: envelope.message_type,
                action_uuid: envelope.action_uuid,
            })
            .map_err(|_| WsTransportError::EventChannelClosed)?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<TransportEvent> {
        self.events_tx.subscribe()
    }
}

#[derive(Debug, Default)]
struct NativeState {
    connected: bool,
    running: bool,
    last_config: Option<WsTransportConfig>,
}

struct NativeRuntime {
    outbound_tx: mpsc::Sender<OutboundEnvelope>,
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
pub struct NativeWsTransport {
    state: Arc<Mutex<NativeState>>,
    runtime: Arc<Mutex<Option<NativeRuntime>>>,
    events_tx: broadcast::Sender<TransportEvent>,
}

impl NativeWsTransport {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(2048);
        Self {
            state: Arc::new(Mutex::new(NativeState::default())),
            runtime: Arc::new(Mutex::new(None)),
            events_tx,
        }
    }

    pub async fn is_connected(&self) -> bool {
        self.state.lock().await.connected
    }
}

impl Default for NativeWsTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WsTransport for NativeWsTransport {
    async fn connect(&self, config: WsTransportConfig) -> Result<(), WsTransportError> {
        let mut runtime_guard = self.runtime.lock().await;
        if runtime_guard.is_some() {
            return Err(WsTransportError::AlreadyRunning);
        }

        {
            let mut state = self.state.lock().await;
            state.running = true;
            state.last_config = Some(config.clone());
        }

        let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundEnvelope>(512);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let events_tx = self.events_tx.clone();
        let state = Arc::clone(&self.state);
        let handle = tokio::spawn(async move {
            run_native_transport_loop(config, outbound_rx, shutdown_rx, events_tx, state).await;
        });

        *runtime_guard = Some(NativeRuntime {
            outbound_tx,
            shutdown_tx,
            handle,
        });
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), WsTransportError> {
        let runtime = self
            .runtime
            .lock()
            .await
            .take()
            .ok_or(WsTransportError::NotConnected)?;

        let _ = runtime.shutdown_tx.send(true);

        runtime
            .handle
            .await
            .map_err(|err| WsTransportError::Join(err.to_string()))?;

        let mut state = self.state.lock().await;
        state.connected = false;
        state.running = false;
        Ok(())
    }

    async fn send(&self, envelope: OutboundEnvelope) -> Result<(), WsTransportError> {
        let outbound_tx = self
            .runtime
            .lock()
            .await
            .as_ref()
            .map(|runtime| runtime.outbound_tx.clone())
            .ok_or(WsTransportError::NotConnected)?;

        outbound_tx
            .send(envelope)
            .await
            .map_err(|_| WsTransportError::TransportChannelClosed)
    }

    fn subscribe(&self) -> broadcast::Receiver<TransportEvent> {
        self.events_tx.subscribe()
    }
}

async fn run_native_transport_loop(
    config: WsTransportConfig,
    mut outbound_rx: mpsc::Receiver<OutboundEnvelope>,
    mut shutdown_rx: watch::Receiver<bool>,
    events_tx: broadcast::Sender<TransportEvent>,
    state: Arc<Mutex<NativeState>>,
) {
    let base_backoff = config.reconnect_backoff_ms.max(200);
    let max_backoff = config.reconnect_backoff_max_ms.max(base_backoff);
    let mut backoff = base_backoff;
    let mut reconnect_attempt: u32 = 0;
    let mut msg_id: u32 = 0;

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let connect_result = tokio::time::timeout(
            Duration::from_millis(config.connect_timeout_ms.max(1000)),
            connect_async_with_config(
                &config.endpoint_url,
                Some(WebSocketConfig::default()),
                false,
            ),
        )
        .await;

        let (mut ws, _) = match connect_result {
            Ok(Ok((ws, response))) => (ws, response),
            Ok(Err(err)) => {
                if handle_reconnect_delay(
                    &events_tx,
                    &mut shutdown_rx,
                    &mut reconnect_attempt,
                    &mut backoff,
                    max_backoff,
                    format!("connect_error:{err}"),
                )
                .await
                {
                    break;
                }
                continue;
            }
            Err(_) => {
                if handle_reconnect_delay(
                    &events_tx,
                    &mut shutdown_rx,
                    &mut reconnect_attempt,
                    &mut backoff,
                    max_backoff,
                    "connect_timeout".to_string(),
                )
                .await
                {
                    break;
                }
                continue;
            }
        };

        reconnect_attempt = 0;
        backoff = base_backoff;

        {
            let mut guard = state.lock().await;
            guard.connected = true;
        }
        emit(&events_tx, TransportEvent::Connected);

        if let Some(jwt_qws) = config.jwt_qws.as_ref() {
            if let Err(err) = send_authenticate(&mut ws, &mut msg_id, jwt_qws).await {
                emit(
                    &events_tx,
                    TransportEvent::TransportError {
                        stage: "authenticate".to_string(),
                        message: err.to_string(),
                    },
                );
                let _ = ws.close(None).await;
                {
                    let mut guard = state.lock().await;
                    guard.connected = false;
                }
                emit(&events_tx, TransportEvent::Disconnected);
                if handle_reconnect_delay(
                    &events_tx,
                    &mut shutdown_rx,
                    &mut reconnect_attempt,
                    &mut backoff,
                    max_backoff,
                    "authenticate_failed".to_string(),
                )
                .await
                {
                    break;
                }
                continue;
            }
            emit(&events_tx, TransportEvent::Authenticated);
        }

        if config.auto_subscribe {
            if let Err(err) = send_subscribe(
                &mut ws,
                &mut msg_id,
                config.qcloud_proto,
                &config.subscribe_channels,
            )
            .await
            {
                emit(
                    &events_tx,
                    TransportEvent::TransportError {
                        stage: "subscribe".to_string(),
                        message: err.to_string(),
                    },
                );
                let _ = ws.close(None).await;
                {
                    let mut guard = state.lock().await;
                    guard.connected = false;
                }
                emit(&events_tx, TransportEvent::Disconnected);
                if handle_reconnect_delay(
                    &events_tx,
                    &mut shutdown_rx,
                    &mut reconnect_attempt,
                    &mut backoff,
                    max_backoff,
                    "subscribe_failed".to_string(),
                )
                .await
                {
                    break;
                }
                continue;
            }
            emit(&events_tx, TransportEvent::Subscribed);
        }

        let mut keepalive = tokio::time::interval(Duration::from_millis(
            config.keepalive_interval_ms.max(1_000),
        ));
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let disconnect_reason = loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        let _ = ws.close(None).await;
                        break "shutdown".to_string();
                    }
                }
                maybe_envelope = outbound_rx.recv() => {
                    match maybe_envelope {
                        Some(envelope) => {
                            if let Err(err) = send_outbound_payload(
                                &mut ws,
                                &mut msg_id,
                                config.qcloud_proto,
                                &envelope,
                            ).await {
                                break format!("send_error:{err}");
                            }
                            emit(
                                &events_tx,
                                TransportEvent::OutboundSent {
                                    message_type: envelope.message_type,
                                    action_uuid: envelope.action_uuid,
                                },
                            );
                        }
                        None => break "outbound_channel_closed".to_string(),
                    }
                }
                _ = keepalive.tick() => {
                    if let Err(err) = ws.send(WsMessage::Ping(Vec::new().into())).await {
                        break format!("keepalive_ping_error:{err}");
                    }
                    emit(&events_tx, TransportEvent::KeepalivePingSent);
                }
                incoming = ws.next() => {
                    match incoming {
                        Some(Ok(WsMessage::Binary(data))) => {
                            if let Err(err) = handle_incoming_binary(&events_tx, &data) {
                                emit(
                                    &events_tx,
                                    TransportEvent::TransportError {
                                        stage: "decode_inbound_binary".to_string(),
                                        message: err.to_string(),
                                    },
                                );
                            }
                        }
                        Some(Ok(WsMessage::Pong(_))) => {
                            emit(&events_tx, TransportEvent::KeepalivePongReceived);
                        }
                        Some(Ok(WsMessage::Ping(payload))) => {
                            let _ = ws.send(WsMessage::Pong(payload)).await;
                        }
                        Some(Ok(WsMessage::Close(_))) => {
                            break "remote_close".to_string();
                        }
                        Some(Ok(_)) => {}
                        Some(Err(err)) => {
                            break format!("ws_read_error:{err}");
                        }
                        None => {
                            break "ws_stream_end".to_string();
                        }
                    }
                }
            }
        };

        {
            let mut guard = state.lock().await;
            guard.connected = false;
        }
        emit(&events_tx, TransportEvent::Disconnected);

        if *shutdown_rx.borrow() {
            break;
        }

        if handle_reconnect_delay(
            &events_tx,
            &mut shutdown_rx,
            &mut reconnect_attempt,
            &mut backoff,
            max_backoff,
            disconnect_reason,
        )
        .await
        {
            break;
        }
    }

    let mut guard = state.lock().await;
    guard.connected = false;
    guard.running = false;
}

async fn handle_reconnect_delay(
    events_tx: &broadcast::Sender<TransportEvent>,
    shutdown_rx: &mut watch::Receiver<bool>,
    reconnect_attempt: &mut u32,
    backoff_ms: &mut u64,
    max_backoff: u64,
    reason: String,
) -> bool {
    *reconnect_attempt = reconnect_attempt.saturating_add(1);

    emit(
        events_tx,
        TransportEvent::ReconnectScheduled {
            attempt: *reconnect_attempt,
            backoff_ms: *backoff_ms,
            reason,
        },
    );

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(*backoff_ms)) => {}
        _ = shutdown_rx.changed() => {
            if *shutdown_rx.borrow() {
                return true;
            }
        }
    }

    *backoff_ms = (*backoff_ms).saturating_mul(2).min(max_backoff);
    false
}

fn handle_incoming_binary(
    events_tx: &broadcast::Sender<TransportEvent>,
    data: &[u8],
) -> Result<(), WsTransportError> {
    let (cloud_message_type, payload) = decode_qcloud_frame(data)?;

    emit(
        events_tx,
        TransportEvent::InboundFrameDecoded {
            cloud_message_type,
            payload_size: payload.len(),
        },
    );

    match cloud_message_type {
        MSG_TYPE_PAYLOAD => {
            let cloud_payload = CloudPayload::decode(payload).map_err(|err| {
                WsTransportError::Protocol(format!("decode payload envelope: {err}"))
            })?;

            let inner_payload = cloud_payload.payload.unwrap_or_default();
            emit(
                events_tx,
                TransportEvent::InboundPayloadBytes {
                    cloud_message_type,
                    payload: inner_payload.clone(),
                },
            );

            // The server wraps QConnectMessages inside an inner envelope with:
            //   field 1 (fixed64): correlation ID
            //   field 2 (varint): sequence number
            //   field 3 (bytes): the actual QConnectMessage protobuf
            // We must unwrap field 3 before passing to the decoders.
            let qconnect_bytes = match QCloudInnerEnvelope::decode(inner_payload.as_slice()) {
                Ok(envelope) => {
                    let msg = envelope.message.unwrap_or_default();
                    log::info!(
                        "[QConnect/Decode] Envelope unwrapped: corr={:?} seq={:?} message_len={}",
                        envelope.correlation_id,
                        envelope.sequence,
                        msg.len()
                    );
                    msg
                }
                Err(err) => {
                    log::warn!(
                        "[QConnect/Decode] Envelope decode failed ({}), using raw {} bytes",
                        err,
                        inner_payload.len()
                    );
                    inner_payload
                }
            };

            match decode_queue_server_events(&qconnect_bytes) {
                Ok(events) => {
                    log::info!("[QConnect/Decode] Queue events decoded: {}", events.len());
                    for event in events {
                        emit(events_tx, TransportEvent::InboundQueueServerEvent(event));
                    }
                }
                Err(err) => {
                    log::warn!("[QConnect/Decode] Queue events decode error: {}", err);
                }
            }

            match decode_renderer_server_commands(&qconnect_bytes) {
                Ok(commands) => {
                    if !commands.is_empty() {
                        log::info!("[QConnect/Decode] Renderer commands decoded: {}", commands.len());
                    }
                    for command in commands {
                        emit(
                            events_tx,
                            TransportEvent::InboundRendererServerCommand(command),
                        );
                    }
                }
                Err(err) => {
                    log::debug!("[QConnect/Decode] Renderer commands decode error: {}", err);
                }
            }

            if let Ok(inbound) = decode_inbound_json(&qconnect_bytes) {
                emit(events_tx, TransportEvent::InboundReceived(inbound));
            }
        }
        MSG_TYPE_ERROR => {
            emit(
                events_tx,
                TransportEvent::TransportError {
                    stage: "qcloud_error_frame".to_string(),
                    message: format!("bytes={}", payload.len()),
                },
            );
        }
        MSG_TYPE_DISCONNECT => {
            emit(
                events_tx,
                TransportEvent::TransportError {
                    stage: "qcloud_disconnect_frame".to_string(),
                    message: "remote_disconnect_signal".to_string(),
                },
            );
        }
        _ => {}
    }

    Ok(())
}

async fn send_authenticate<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    msg_id: &mut u32,
    jwt_qws: &str,
) -> Result<(), WsTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let auth = Authenticate {
        msg_id: Some(next_msg_id(msg_id)),
        msg_date: Some(now_ms()),
        jwt: Some(jwt_qws.to_string()),
    };
    let frame = encode_qcloud_frame(MSG_TYPE_AUTHENTICATE, &auth.encode_to_vec());
    ws.send(WsMessage::Binary(frame.into()))
        .await
        .map_err(|err| WsTransportError::Protocol(format!("send authenticate: {err}")))
}

async fn send_subscribe<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    msg_id: &mut u32,
    qcloud_proto: u32,
    channels: &[Vec<u8>],
) -> Result<(), WsTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let subscribe = Subscribe {
        msg_id: Some(next_msg_id(msg_id)),
        msg_date: Some(now_ms()),
        proto: Some(qcloud_proto),
        channels: channels.to_vec(),
    };

    let frame = encode_qcloud_frame(MSG_TYPE_SUBSCRIBE, &subscribe.encode_to_vec());
    ws.send(WsMessage::Binary(frame.into()))
        .await
        .map_err(|err| WsTransportError::Protocol(format!("send subscribe: {err}")))
}

async fn send_outbound_payload<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
    msg_id: &mut u32,
    qcloud_proto: u32,
    envelope: &OutboundEnvelope,
) -> Result<(), WsTransportError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload_bytes = encode_outbound_payload_bytes(envelope)
        .map_err(|err| WsTransportError::Serialization(err.to_string()))?;

    let payload = CloudPayload {
        msg_id: Some(next_msg_id(msg_id)),
        msg_date: Some(now_ms()),
        proto: Some(qcloud_proto),
        src: None,
        dests: Vec::new(),
        payload: Some(payload_bytes),
    };

    let frame = encode_qcloud_frame(MSG_TYPE_PAYLOAD, &payload.encode_to_vec());
    ws.send(WsMessage::Binary(frame.into()))
        .await
        .map_err(|err| WsTransportError::Protocol(format!("send payload: {err}")))
}

fn encode_qcloud_frame(msg_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + 10 + payload.len());
    frame.push(msg_type);
    encode_varint(payload.len() as u64, &mut frame);
    frame.extend_from_slice(payload);
    frame
}

fn decode_qcloud_frame(data: &[u8]) -> Result<(u8, &[u8]), WsTransportError> {
    if data.is_empty() {
        return Err(WsTransportError::Protocol("empty qcloud frame".to_string()));
    }

    let msg_type = data[0];
    let mut cursor = &data[1..];
    let payload_len = decode_varint(&mut cursor)
        .map_err(|err| WsTransportError::Protocol(format!("decode varint length: {err}")))?
        as usize;

    let consumed_varint = data.len().saturating_sub(1 + cursor.len());
    let payload_start = 1 + consumed_varint;

    if data.len() < payload_start + payload_len {
        return Err(WsTransportError::Protocol(format!(
            "truncated qcloud frame: expected={}, got={}",
            payload_start + payload_len,
            data.len()
        )));
    }

    Ok((msg_type, &data[payload_start..payload_start + payload_len]))
}

fn emit(events_tx: &broadcast::Sender<TransportEvent>, event: TransportEvent) {
    let _ = events_tx.send(event);
}

fn next_msg_id(msg_id: &mut u32) -> u32 {
    *msg_id = msg_id.saturating_add(1);
    *msg_id
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct Authenticate {
    #[prost(uint32, optional, tag = "1")]
    pub msg_id: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub msg_date: Option<u64>,
    #[prost(string, optional, tag = "3")]
    pub jwt: Option<String>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct Subscribe {
    #[prost(uint32, optional, tag = "1")]
    pub msg_id: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub msg_date: Option<u64>,
    #[prost(uint32, optional, tag = "3")]
    pub proto: Option<u32>,
    #[prost(bytes = "vec", repeated, tag = "4")]
    pub channels: Vec<Vec<u8>>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
struct CloudPayload {
    #[prost(uint32, optional, tag = "1")]
    pub msg_id: Option<u32>,
    #[prost(uint64, optional, tag = "2")]
    pub msg_date: Option<u64>,
    #[prost(uint32, optional, tag = "3")]
    pub proto: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "4")]
    pub src: Option<Vec<u8>>,
    #[prost(bytes = "vec", repeated, tag = "5")]
    pub dests: Vec<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "7")]
    pub payload: Option<Vec<u8>>,
}

/// Inner envelope that wraps QConnectMessage inside CloudPayload.payload.
/// The server sends: CloudPayload { payload: QCloudInnerEnvelope { message: QConnectMessage } }
#[derive(Clone, PartialEq, ::prost::Message)]
struct QCloudInnerEnvelope {
    #[prost(fixed64, optional, tag = "1")]
    pub correlation_id: Option<u64>,
    #[prost(uint64, optional, tag = "2")]
    pub sequence: Option<u64>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub message: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qcloud_frame_roundtrip() {
        let payload = vec![0x01, 0x02, 0x03, 0x04];
        let encoded = encode_qcloud_frame(MSG_TYPE_PAYLOAD, &payload);
        let (msg_type, decoded_payload) = decode_qcloud_frame(&encoded).expect("decode frame");
        assert_eq!(msg_type, MSG_TYPE_PAYLOAD);
        assert_eq!(decoded_payload, payload.as_slice());
    }

    #[test]
    fn qcloud_frame_rejects_truncated_payload() {
        let payload = vec![0x0a, 0x0b, 0x0c];
        let mut encoded = encode_qcloud_frame(MSG_TYPE_PAYLOAD, &payload);
        encoded.truncate(encoded.len() - 1);
        let err = decode_qcloud_frame(&encoded).expect_err("expected truncation error");
        assert!(matches!(err, WsTransportError::Protocol(_)));
    }

    #[test]
    fn cloud_payload_proto_roundtrip() {
        let payload = CloudPayload {
            msg_id: Some(12),
            msg_date: Some(34),
            proto: Some(1),
            src: None,
            dests: vec![vec![1, 2, 3]],
            payload: Some(vec![9, 8, 7]),
        };
        let encoded = payload.encode_to_vec();
        let decoded = CloudPayload::decode(encoded.as_slice()).expect("decode cloud payload");
        assert_eq!(decoded.msg_id, Some(12));
        assert_eq!(decoded.proto, Some(1));
        assert_eq!(decoded.payload, Some(vec![9, 8, 7]));
    }
}
