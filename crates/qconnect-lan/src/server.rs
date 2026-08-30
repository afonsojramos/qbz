use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use socket2::{Domain, Protocol, Socket, Type};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::admission::{admission_channel, AdmissionInbox, AdmissionSender, SubmitError};
use crate::mdns::MdnsRegistration;
use crate::validation::{parse_and_validate, EndpointPolicy, ValidationError, MAX_BODY_BYTES};
use crate::LanProjection;

pub const SERVICE_TYPE: &str = "_qobuz-connect._tcp.local.";
pub const DEFAULT_SDK_VERSION: &str = "0.9.5";

const POST_RATE_PER_MINUTE: f64 = 6.0;
const POST_BURST: f64 = 2.0;
const MAX_RATE_ORIGINS: usize = 256;
const HTTP_WORKERS: usize = 2;
pub const DEFAULT_LOCAL_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("qconnect-lan-bind")]
    Bind,
    #[error("qconnect-lan-address-discovery")]
    AddressDiscovery,
    #[error("qconnect-lan-no-addresses")]
    NoLanAddresses,
    #[error("qconnect-lan-mdns")]
    Mdns,
    #[error("qconnect-lan-config")]
    InvalidConfig,
}

impl From<mdns_sd::Error> for LanError {
    fn from(_: mdns_sd::Error) -> Self {
        Self::Mdns
    }
}

#[derive(Clone)]
pub struct LanServiceConfig {
    pub bind_addr: SocketAddr,
    pub projection: LanProjection,
    pub endpoint_policy: EndpointPolicy,
    pub device_uuid: String,
    pub sdk_version: String,
    /// Bounds stalled local request reads. The production default is the
    /// contract's five-second local read/parse budget.
    pub read_timeout: Duration,
    /// Tests and environments without multicast can exercise the exact HTTP
    /// wire while keeping discovery disabled. Production adapters leave true.
    pub advertise_mdns: bool,
    /// Optional explicit address set, primarily for deterministic integration
    /// tests. Production uses interface discovery and mdns-sd auto updates.
    pub advertised_addresses: Option<Vec<IpAddr>>,
}

impl LanServiceConfig {
    pub fn new(
        projection: LanProjection,
        endpoint_policy: EndpointPolicy,
        device_uuid: impl Into<String>,
    ) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            projection,
            endpoint_policy,
            device_uuid: device_uuid.into(),
            sdk_version: DEFAULT_SDK_VERSION.to_string(),
            read_timeout: DEFAULT_LOCAL_READ_TIMEOUT,
            advertise_mdns: true,
            advertised_addresses: None,
        }
    }
}

pub struct LanService {
    port: u16,
    server: Arc<Server>,
    admission: AdmissionSender,
    accepting: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
    worker_threads: Vec<JoinHandle<()>>,
    mdns: Option<MdnsRegistration>,
}

impl LanService {
    /// Bind first, spawn HTTP workers, then publish DNS-SD. On any discovery
    /// failure the bound listener is torn down before the error returns.
    pub fn start(config: LanServiceConfig) -> Result<(Self, AdmissionInbox), LanError> {
        validate_config(&config)?;
        let server = Arc::new(bind_http(config.bind_addr, config.read_timeout)?);
        let port = server
            .server_addr()
            .to_ip()
            .map(|address| address.port())
            .ok_or(LanError::Bind)?;
        let (admission, inbox) = admission_channel();
        let accepting = Arc::new(AtomicBool::new(true));
        let rate_limiter = Arc::new(Mutex::new(PostRateLimiter::default()));

        // Count queued plus active work explicitly. This avoids a scheduling
        // race where a worker exists but has not reached `recv` yet.
        let (request_tx, request_rx) = mpsc::sync_channel::<Request>(HTTP_WORKERS);
        let request_rx = Arc::new(Mutex::new(request_rx));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let mut worker_threads = Vec::with_capacity(HTTP_WORKERS);
        for _ in 0..HTTP_WORKERS {
            let worker_rx = Arc::clone(&request_rx);
            let worker_accepting = Arc::clone(&accepting);
            let worker_admission = admission.clone();
            let worker_projection = config.projection.clone();
            let worker_policy = config.endpoint_policy.clone();
            let worker_limiter = Arc::clone(&rate_limiter);
            let worker_in_flight = Arc::clone(&in_flight);
            worker_threads.push(std::thread::spawn(move || loop {
                let request = worker_rx
                    .lock()
                    .expect("LAN request queue lock poisoned")
                    .recv();
                let request = match request {
                    Ok(request) => request,
                    Err(_) => break,
                };
                if worker_accepting.load(Ordering::Acquire) {
                    handle_request(
                        request,
                        &worker_projection,
                        &worker_policy,
                        &worker_admission,
                        &worker_limiter,
                    );
                } else {
                    let _ = request.respond(error_response(StatusCode(503), "unavailable"));
                }
                worker_in_flight.fetch_sub(1, Ordering::AcqRel);
            }));
        }

        let thread_server = Arc::clone(&server);
        let thread_accepting = Arc::clone(&accepting);
        let accept_in_flight = Arc::clone(&in_flight);
        let accept_thread = std::thread::spawn(move || {
            while thread_accepting.load(Ordering::Acquire) {
                match thread_server.recv_timeout(Duration::from_millis(250)) {
                    Ok(Some(request)) => {
                        let admitted = accept_in_flight
                            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                                (current < HTTP_WORKERS).then_some(current + 1)
                            })
                            .is_ok();
                        if !admitted {
                            let _ = request.respond(error_response(StatusCode(429), "busy"));
                            continue;
                        }
                        match request_tx.try_send(request) {
                            Ok(()) => {}
                            Err(TrySendError::Full(request)) => {
                                accept_in_flight.fetch_sub(1, Ordering::AcqRel);
                                let _ = request.respond(error_response(StatusCode(429), "busy"));
                            }
                            Err(TrySendError::Disconnected(request)) => {
                                accept_in_flight.fetch_sub(1, Ordering::AcqRel);
                                let _ =
                                    request.respond(error_response(StatusCode(503), "unavailable"));
                                break;
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(_) if !thread_accepting.load(Ordering::Acquire) => break,
                    // Malformed or timed-out client connections are surfaced
                    // by tiny_http through this queue too; they must not kill
                    // discovery for every other controller.
                    Err(_) => {}
                }
            }
        });

        let mdns = if config.advertise_mdns {
            match MdnsRegistration::register(
                &config.device_uuid,
                port,
                &config.sdk_version,
                config.advertised_addresses,
            ) {
                Ok(mdns) => Some(mdns),
                Err(error) => {
                    admission.close();
                    accepting.store(false, Ordering::Release);
                    server.unblock();
                    let _ = accept_thread.join();
                    for worker in worker_threads {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        } else {
            None
        };

        Ok((
            Self {
                port,
                server,
                admission,
                accepting,
                accept_thread: Some(accept_thread),
                worker_threads,
                mdns,
            },
            inbox,
        ))
    }

    pub const fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown(&mut self) {
        self.admission.close();
        if let Some(mdns) = self.mdns.take() {
            mdns.shutdown();
        }
        self.accepting.store(false, Ordering::Release);
        self.server.unblock();
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        for thread in self.worker_threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for LanService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn validate_config(config: &LanServiceConfig) -> Result<(), LanError> {
    let display = config.projection.display_info();
    let connect = config.projection.connect_info();
    let required = [
        display.friendly_name.as_str(),
        display.serial_number.as_str(),
        display.brand_display_name.as_str(),
        display.model_display_name.as_str(),
        display.software_version.as_str(),
        connect.app_id.as_str(),
        config.device_uuid.as_str(),
        config.sdk_version.as_str(),
    ];
    if config.bind_addr.is_ipv6()
        || config.read_timeout.is_zero()
        || required.iter().any(|value| value.trim().is_empty())
        || display.serial_number != config.device_uuid
    {
        return Err(LanError::InvalidConfig);
    }
    Ok(())
}

fn bind_http(bind_addr: SocketAddr, read_timeout: Duration) -> Result<Server, LanError> {
    let domain = if bind_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket =
        Socket::new(domain, Type::STREAM, Some(Protocol::TCP)).map_err(|_| LanError::Bind)?;
    socket.set_reuse_address(true).map_err(|_| LanError::Bind)?;
    if bind_addr.is_ipv6() {
        socket.set_only_v6(true).map_err(|_| LanError::Bind)?;
    }
    // Linux propagates SO_RCVTIMEO from a listening socket to accepted TCP
    // sockets. That bounds stalled request reads inside tiny_http.
    socket
        .set_read_timeout(Some(read_timeout))
        .map_err(|_| LanError::Bind)?;
    socket.bind(&bind_addr.into()).map_err(|_| LanError::Bind)?;
    socket.listen(128).map_err(|_| LanError::Bind)?;
    let listener: TcpListener = socket.into();
    Server::from_listener(listener, None).map_err(|_| LanError::Bind)
}

fn handle_request(
    mut request: Request,
    projection: &LanProjection,
    policy: &EndpointPolicy,
    admission: &AdmissionSender,
    rate_limiter: &Arc<Mutex<PostRateLimiter>>,
) {
    let method = request.method().clone();
    let path = request.url().to_string();
    let response = match path.as_str() {
        "/get-display-info" if method == Method::Get => {
            json_response(StatusCode(200), &projection.display_info())
        }
        "/get-connect-info" if method == Method::Get => {
            json_response(StatusCode(200), &projection.connect_info())
        }
        "/connect-to-qconnect" if method == Method::Post => {
            handle_connect(&mut request, policy, admission, rate_limiter)
        }
        "/get-display-info" | "/get-connect-info" | "/connect-to-qconnect" => {
            error_response(StatusCode(405), "method_not_allowed")
        }
        _ => error_response(StatusCode(404), "not_found"),
    };
    let _ = request.respond(response);
}

fn handle_connect(
    request: &mut Request,
    policy: &EndpointPolicy,
    admission: &AdmissionSender,
    rate_limiter: &Arc<Mutex<PostRateLimiter>>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    if !has_json_content_type(request) {
        return error_response(StatusCode(415), "json_required");
    }
    if declared_body_too_large(request) {
        return error_response(StatusCode(413), "body_too_large");
    }
    let remote_ip = request.remote_addr().map(SocketAddr::ip);
    if !rate_limiter
        .lock()
        .expect("LAN rate-limit lock poisoned")
        .allow(remote_ip, Instant::now())
    {
        return error_response(StatusCode(429), "rate_limited");
    }

    let mut body = Vec::new();
    if request
        .as_reader()
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
    {
        return error_response(StatusCode(400), "body_read_failed");
    }
    if body.len() > MAX_BODY_BYTES {
        return error_response(StatusCode(413), "body_too_large");
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;
    let candidate = match parse_and_validate(&body, policy, now) {
        Ok(candidate) => candidate,
        Err(ValidationError::BecomeActiveRequired) => {
            return error_response(StatusCode(422), "active_required")
        }
        Err(_) => return error_response(StatusCode(400), "invalid_handoff"),
    };

    match admission.submit(candidate) {
        Ok(()) => json_response(StatusCode(200), &serde_json::json!({})),
        Err(SubmitError::Closed) => error_response(StatusCode(503), "unavailable"),
    }
}

fn has_json_content_type(request: &Request) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Content-Type")
            && header
                .value
                .as_str()
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn declared_body_too_large(request: &Request) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Content-Length")
            && header
                .value
                .as_str()
                .trim()
                .parse::<usize>()
                .is_ok_and(|length| length > MAX_BODY_BYTES)
    })
}

fn json_response<T: serde::Serialize>(
    status: StatusCode,
    value: &T,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    Response::from_data(body)
        .with_status_code(status)
        .with_header(json_content_type())
}

fn error_response(status: StatusCode, code: &'static str) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(status, &serde_json::json!({ "error": code }))
}

fn json_content_type() -> Header {
    Header::from_bytes("Content-Type", "application/json").expect("static JSON content-type header")
}

#[derive(Debug, Clone, Copy)]
struct RateBucket {
    tokens: f64,
    updated_at: Instant,
}

#[derive(Default)]
struct PostRateLimiter {
    buckets: HashMap<Option<IpAddr>, RateBucket>,
}

impl PostRateLimiter {
    fn allow(&mut self, origin: Option<IpAddr>, now: Instant) -> bool {
        if self.buckets.len() >= MAX_RATE_ORIGINS && !self.buckets.contains_key(&origin) {
            if let Some(oldest) = self
                .buckets
                .iter()
                .min_by_key(|(_, bucket)| bucket.updated_at)
                .map(|(origin, _)| *origin)
            {
                self.buckets.remove(&oldest);
            }
        }
        let bucket = self.buckets.entry(origin).or_insert(RateBucket {
            tokens: POST_BURST,
            updated_at: now,
        });
        let elapsed = now
            .saturating_duration_since(bucket.updated_at)
            .as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * POST_RATE_PER_MINUTE / 60.0).min(POST_BURST);
        bucket.updated_at = now;
        if bucket.tokens < 1.0 {
            return false;
        }
        bucket.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_rate_limit_allows_burst_then_refills() {
        let mut limiter = PostRateLimiter::default();
        let now = Instant::now();
        let ip = Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(limiter.allow(ip, now));
        assert!(limiter.allow(ip, now));
        assert!(!limiter.allow(ip, now));
        assert!(limiter.allow(ip, now + Duration::from_secs(10)));
    }
}
