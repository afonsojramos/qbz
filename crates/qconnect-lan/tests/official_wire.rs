use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qconnect_lan::{
    ConnectInfo, DeviceType, DisplayInfo, EndpointPolicy, LanError, LanProjection, LanService,
    LanServiceConfig, MaxAudioQuality, MAX_BODY_BYTES,
};
use reqwest::StatusCode;

const DEVICE_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

fn start_service() -> (
    LanService,
    qconnect_lan::AdmissionInbox,
    String,
    LanProjection,
) {
    start_service_with_timeout(Duration::from_secs(5))
}

fn start_service_with_timeout(
    read_timeout: Duration,
) -> (
    LanService,
    qconnect_lan::AdmissionInbox,
    String,
    LanProjection,
) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let projection = LanProjection::new(
        DisplayInfo {
            friendly_name: "Kitchen".to_string(),
            serial_number: DEVICE_UUID.to_string(),
            brand_display_name: "QBZ".to_string(),
            model_display_name: "QBZ Daemon".to_string(),
            max_audio_quality: MaxAudioQuality::UpToHires192,
            device_type: DeviceType::Streamer,
            software_version: "2.1.0".to_string(),
        },
        ConnectInfo {
            app_id: "trusted-app-id".to_string(),
            current_session_id: None,
        },
    );
    let mut config = LanServiceConfig::new(
        projection.clone(),
        EndpointPolicy::new(["api.qobuz.test"], ["qws.qobuz.test"]),
        DEVICE_UUID,
    );
    config.bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    config.read_timeout = read_timeout;
    config.advertise_mdns = false;
    let (service, inbox) = LanService::start(config).unwrap();
    let base = format!("http://127.0.0.1:{}", service.port());
    (service, inbox, base, projection)
}

fn test_config(bind_addr: SocketAddr) -> LanServiceConfig {
    let projection = LanProjection::new(
        DisplayInfo {
            friendly_name: "Kitchen".to_string(),
            serial_number: DEVICE_UUID.to_string(),
            brand_display_name: "QBZ".to_string(),
            model_display_name: "QBZ Daemon".to_string(),
            max_audio_quality: MaxAudioQuality::UpToHires192,
            device_type: DeviceType::Streamer,
            software_version: "2.1.0".to_string(),
        },
        ConnectInfo {
            app_id: "trusted-app-id".to_string(),
            current_session_id: None,
        },
    );
    let mut config = LanServiceConfig::new(
        projection,
        EndpointPolicy::new(["api.qobuz.test"], ["qws.qobuz.test"]),
        DEVICE_UUID,
    );
    config.bind_addr = bind_addr;
    config
}

fn stalled_post(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "POST /connect-to-qconnect HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 2048\r\nConnection: close\r\n\r\n{{"
    )
    .unwrap();
    stream.flush().unwrap();
    stream
}

fn handoff(session_id: &str, become_active: bool) -> serde_json::Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    serde_json::json!({
        "session_id": session_id,
        "jwt_api": {
            "endpoint": "https://api.qobuz.test/api.json/0.2",
            "exp": now + 600,
            "jwt": "api-secret"
        },
        "jwt_qconnect": {
            "endpoint": "wss://qws.qobuz.test/ws",
            "exp": now + 600,
            "jwt": "qws-secret"
        },
        "become_active": become_active
    })
}

#[tokio::test]
async fn serves_official_get_union_and_admits_exact_post_shape() {
    let (mut service, inbox, base, _) = start_service();
    let client = reqwest::Client::new();

    let display = client
        .get(format!("{base}/get-display-info"))
        .header("Connection", "close")
        .send()
        .await
        .unwrap();
    assert_eq!(display.status(), StatusCode::OK);
    assert_eq!(
        display.headers()["content-type"].to_str().unwrap(),
        "application/json"
    );
    assert_eq!(
        display.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({
            "friendly_name": "Kitchen",
            "serial_number": DEVICE_UUID,
            "brand_display_name": "QBZ",
            "model_display_name": "QBZ Daemon",
            "max_audio_quality": "UP_TO_HIRES_192",
            "type": "Streamer",
            "software_version": "2.1.0"
        })
    );

    let connect = client
        .get(format!("{base}/get-connect-info"))
        .send()
        .await
        .unwrap();
    assert_eq!(connect.status(), StatusCode::OK);
    assert_eq!(
        connect.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({ "app_id": "trusted-app-id", "current_session_id": null })
    );

    let accepted = client
        .post(format!("{base}/connect-to-qconnect"))
        .json(&handoff("controller-session", true))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(
        accepted.json::<serde_json::Value>().await.unwrap(),
        serde_json::json!({})
    );
    let candidate = inbox.take_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(candidate.session_id(), "controller-session");
    assert_eq!(
        candidate.api_token().endpoint(),
        "https://api.qobuz.test/api.json/0.2"
    );
    assert_eq!(
        candidate.qconnect_token().endpoint(),
        "wss://qws.qobuz.test/ws"
    );

    service.shutdown();
    service.shutdown();
    assert!(inbox.is_closed());
}

#[tokio::test]
async fn pending_post_is_latest_wins_and_rate_limit_is_bounded() {
    let (mut service, inbox, base, _) = start_service();
    let client = reqwest::Client::new();

    for session in ["first", "second"] {
        let response = client
            .post(format!("{base}/connect-to-qconnect"))
            .json(&handoff(session, true))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(inbox.try_take().unwrap().session_id(), "second");

    let limited = client
        .post(format!("{base}/connect-to-qconnect"))
        .json(&handoff("third", true))
        .send()
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(!limited.text().await.unwrap().contains("api-secret"));
    service.shutdown();
}

#[tokio::test]
async fn rejects_noncanonical_http_and_oversized_or_inactive_handoffs() {
    let (mut service, _inbox, base, _) = start_service();
    let client = reqwest::Client::new();

    assert_eq!(
        client
            .get(format!("{base}/get-display-info?alias=1"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .post(format!("{base}/get-connect-info"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        client
            .post(format!("{base}/connect-to-qconnect"))
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        client
            .post(format!("{base}/connect-to-qconnect"))
            .json(&handoff("inactive", false))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let (mut oversized_service, _, oversized_base, _) = start_service();
    let oversized = client
        .post(format!("{oversized_base}/connect-to-qconnect"))
        .header("Content-Type", "application/json")
        .body(vec![b'x'; MAX_BODY_BYTES + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);

    service.shutdown();
    oversized_service.shutdown();
}

#[tokio::test]
async fn connect_projection_updates_without_restarting_listener() {
    let (mut service, _, base, projection) = start_service();
    projection.set_current_session_id(Some("active-session".to_string()));

    let value = reqwest::get(format!("{base}/get-connect-info"))
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(value["current_session_id"], "active-session");
    service.shutdown();
}

#[tokio::test]
async fn stalled_bodies_time_out_and_concurrency_stays_bounded() {
    let (mut service, _, base, _) = start_service_with_timeout(Duration::from_millis(500));
    let mut first = stalled_post(service.port());
    let second = stalled_post(service.port());
    let deadline = std::time::Instant::now() + Duration::from_millis(250);
    loop {
        let response = reqwest::get(format!("{base}/get-display-info"))
            .await
            .unwrap();
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            break;
        }
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            std::time::Instant::now() < deadline,
            "workers never saturated"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let mut response = String::new();
    first.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");

    drop(second);
    service.shutdown();
}

#[test]
fn mdns_failure_after_bind_releases_listener_and_never_advertises_loopback() {
    let reservation = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let bind_addr = reservation.local_addr().unwrap();
    drop(reservation);

    let mut config = test_config(bind_addr);
    config.advertise_mdns = true;
    config.advertised_addresses = Some(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
    let error = match LanService::start(config) {
        Ok(_) => panic!("loopback mDNS address unexpectedly accepted"),
        Err(error) => error,
    };
    assert!(matches!(error, LanError::NoLanAddresses));

    std::thread::sleep(Duration::from_millis(25));
    assert!(TcpStream::connect_timeout(&bind_addr, Duration::from_millis(100)).is_err());
}
