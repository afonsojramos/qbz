use qbz_jellyfin::{JellyfinError, QuickConnect};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(
    responses: Vec<(&'static str, u16, &'static str)>,
) -> (String, tokio::task::JoinHandle<()>) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/jellyfin", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for (path, status, body) in responses {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| {
                            line.to_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|s| s.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert_eq!(request.lines().next().unwrap(), path);
            if path.starts_with("POST") {
                assert!(request.contains("DeviceId=\"stable-install\""));
            }
            if path.contains("AuthenticateWithQuickConnect") {
                assert!(request.contains(r#"{"Secret":"private-secret"}"#));
                assert!(!request.contains("123456"));
            }
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    (url, task)
}
const ENABLED: (&str, u16, &str) = ("GET /jellyfin/QuickConnect/Enabled HTTP/1.1", 200, "true");
const INIT: (&str, u16, &str) = (
    "POST /jellyfin/QuickConnect/Initiate HTTP/1.1",
    200,
    r#"{"Authenticated":false,"Secret":"private-secret","Code":"123456"}"#,
);
const POLL: &str = "GET /jellyfin/QuickConnect/Connect?secret=private-secret HTTP/1.1";

#[tokio::test]
async fn pairing_pending_then_authorized_exchanges_secret_and_preserves_identity() {
    let (url, task) = server(vec![
        ENABLED,
        INIT,
        (POLL, 200, r#"{"Authenticated":false}"#),
        (POLL, 200, r#"{"Authenticated":true}"#),
        (
            "POST /jellyfin/Users/AuthenticateWithQuickConnect HTTP/1.1",
            200,
            r#"{"AccessToken":"token","ServerId":"server","User":{"Id":"user","Name":"name"}}"#,
        ),
    ])
    .await;
    let pairing = QuickConnect::start(&url, "stable-install").await.unwrap();
    assert_eq!(pairing.code(), "123456");
    assert!(!pairing.authorized().await.unwrap());
    let session = pairing
        .wait_for_authorization(Duration::from_secs(9))
        .await
        .unwrap();
    assert_eq!(session.access_token, "token");
    assert_eq!(session.user_id, "user");
    task.await.unwrap();
}
#[tokio::test]
async fn disabled_does_not_initiate() {
    let (url, task) = server(vec![(ENABLED.0, 200, "false")]).await;
    assert!(matches!(
        QuickConnect::start(&url, "stable-install").await,
        Err(JellyfinError::QuickConnectDisabled)
    ));
    task.await.unwrap();
}
#[tokio::test]
async fn expiry_and_disabled_while_waiting_are_distinct() {
    for status in [401, 404] {
        let (url, task) = server(vec![ENABLED, INIT, (POLL, status, "null")]).await;
        let pairing = QuickConnect::start(&url, "stable-install").await.unwrap();
        let err = pairing.authorized().await.unwrap_err();
        assert_eq!(
            err,
            if status == 401 {
                JellyfinError::QuickConnectDisabled
            } else {
                JellyfinError::QuickConnectExpired
            }
        );
        task.await.unwrap();
    }
}
#[tokio::test]
async fn local_deadline_stops_polling() {
    let (url, task) = server(vec![ENABLED, INIT]).await;
    let pairing = QuickConnect::start(&url, "stable-install").await.unwrap();
    assert!(matches!(
        pairing
            .wait_for_authorization(Duration::from_millis(20))
            .await,
        Err(JellyfinError::QuickConnectExpired)
    ));
    task.await.unwrap();
}
#[tokio::test]
async fn malformed_response_does_not_echo_secret() {
    let (url, task) = server(vec![
        ENABLED,
        (INIT.0, 200, r#"{"Authenticated":"private-secret"}"#),
    ])
    .await;
    let error = match QuickConnect::start(&url, "stable-install").await {
        Err(e) => e,
        Ok(_) => panic!("malformed response accepted"),
    };
    assert!(!error.to_string().contains("private-secret"));
    task.await.unwrap();
}

#[tokio::test]
async fn exchange_rejection_is_not_reported_as_disabled() {
    let (url, task) = server(vec![
        ENABLED,
        INIT,
        (
            "POST /jellyfin/Users/AuthenticateWithQuickConnect HTTP/1.1",
            403,
            "null",
        ),
    ])
    .await;
    let pairing = QuickConnect::start(&url, "stable-install").await.unwrap();
    assert!(matches!(
        pairing.authenticate().await,
        Err(JellyfinError::AuthenticationRejected)
    ));
    task.await.unwrap();
}
