//! Normal-renderer wire contract. Exercise the public QBZ connection, not just
//! URI parsing: service versions, SOAP paths/headers/payloads and state replies.
use qbz_cast::{DiscoveredDlnaDevice, DlnaConnection, DlnaMetadata};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

#[derive(Debug)]
struct Call {
    path: String,
    action: String,
    body: String,
}
struct Renderer {
    addr: std::net::SocketAddr,
    done: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Vec<Call>>>,
}
impl Renderer {
    fn new(version: u8, empty_optional_urls: bool, relative_urls: bool) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let stopped = done.clone();
        let worker = std::thread::spawn(move || {
            let mut services = String::new();
            for name in ["AVTransport", "RenderingControl"] {
                let scpd = if empty_optional_urls {
                    String::new()
                } else {
                    format!("/{name}/scpd.xml")
                };
                let event = if empty_optional_urls {
                    String::new()
                } else {
                    format!("/{name}/event")
                };
                services.push_str(&format!("<service><serviceType>urn:schemas-upnp-org:service:{name}:{version}</serviceType><serviceId>urn:upnp-org:serviceId:{name}</serviceId><SCPDURL>{scpd}</SCPDURL><controlURL>/{name}/control?zone=main</controlURL><eventSubURL>{event}</eventSubURL></service>"));
            }
            if relative_urls {
                services = services.replace(">/", ">");
            }
            let description = format!("<root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Reference renderer</friendlyName><manufacturer>Test</manufacturer><modelName>Reference</modelName><UDN>uuid:reference</UDN><serviceList>{services}</serviceList></device></root>");
            let mut calls = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap()
                else {
                    continue;
                };
                let response = if request.method() == &tiny_http::Method::Get {
                    assert_eq!(request.url(), "/device/description.xml");
                    description.clone()
                } else {
                    assert_eq!(request.method(), &tiny_http::Method::Post);
                    let header = request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("SOAPAction"))
                        .unwrap()
                        .value
                        .as_str()
                        .trim_matches('"')
                        .to_string();
                    let (service, action) = header.rsplit_once('#').unwrap();
                    let body = match action {
                        "GetPositionInfo" => {
                            "<RelTime>00:01:23</RelTime><TrackDuration>00:05:00</TrackDuration>"
                        }
                        "GetTransportInfo" => {
                            "<CurrentTransportState>PLAYING</CurrentTransportState>"
                        }
                        "GetMute" => "<CurrentMute>1</CurrentMute>",
                        _ => "",
                    };
                    let response = format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{body}</u:{action}Response></s:Body></s:Envelope>");
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let name = if matches!(action, "SetVolume" | "SetMute" | "GetMute") {
                        "RenderingControl"
                    } else {
                        "AVTransport"
                    };
                    assert_eq!(
                        service,
                        format!("urn:schemas-upnp-org:service:{name}:{version}")
                    );
                    assert_eq!(request.url(), format!("/{name}/control?zone=main"));
                    assert!(
                        body.contains(&format!("<u:{action} xmlns:u=\"{service}\">")),
                        "{body}"
                    );
                    calls.push(Call {
                        path: request.url().into(),
                        action: action.into(),
                        body,
                    });
                    response
                };
                request
                    .respond(tiny_http::Response::from_string(response).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/xml").unwrap(),
                    ))
                    .unwrap();
            }
            calls
        });
        Self {
            addr,
            done,
            worker: Some(worker),
        }
    }
    fn finish(mut self) -> Vec<Call> {
        self.done.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

async fn exercise(version: u8, empty_optional_urls: bool, relative_urls: bool) {
    let renderer = Renderer::new(version, empty_optional_urls, relative_urls);
    let device = DiscoveredDlnaDevice {
        id: "uuid:reference".into(),
        name: "Reference renderer".into(),
        manufacturer: "Test".into(),
        model: "Reference".into(),
        ip: "127.0.0.1".into(),
        url: format!("http://{}/device/description.xml", renderer.addr),
        has_av_transport: true,
        has_rendering_control: true,
    };
    let mut connection = DlnaConnection::connect(device).await.unwrap();
    let uri = "http://127.0.0.1:54321/audio/1?token=test&quality=hires";
    connection
        .load_media(
            uri,
            &DlnaMetadata {
                title: "Title & more".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                artwork_url: None,
                duration_secs: Some(300),
            },
            "audio/flac",
        )
        .await
        .unwrap();
    connection.play().await.unwrap();
    assert!(connection.get_status().is_playing);
    let position = connection.get_position_info().await.unwrap();
    assert_eq!(
        (
            position.position_secs,
            position.duration_secs,
            position.transport_state.as_str()
        ),
        (83, 300, "PLAYING")
    );
    connection.pause().await.unwrap();
    assert!(!connection.get_status().is_playing);
    connection.seek(83).await.unwrap();
    connection.set_volume(0.42).await.unwrap();
    connection.set_volume(0.42).await.unwrap();
    connection.set_mute(true).await.unwrap();
    assert!(connection.get_mute().await.unwrap());
    connection.stop().await.unwrap();
    assert!(connection.get_status().current_uri.is_none());
    let calls = renderer.finish();
    let actions: Vec<_> = calls.iter().map(|c| c.action.as_str()).collect();
    assert_eq!(
        actions,
        [
            "Stop",
            "SetAVTransportURI",
            "Play",
            "GetPositionInfo",
            "GetTransportInfo",
            "Pause",
            "Seek",
            "SetVolume",
            "SetMute",
            "GetMute",
            "Stop"
        ]
    );
    assert!(calls[1].body.contains("&amp;quality=hires"));
    assert!(calls[6].body.contains("<Target>00:01:23</Target>"));
    assert!(calls[7].body.contains("<DesiredVolume>42</DesiredVolume>"));
    assert_eq!(calls[7].path, "/RenderingControl/control?zone=main");
}

#[tokio::test]
async fn normal_renderer_v1_preserves_transport_and_volume_wire_contract() {
    exercise(1, false, false).await;
}
#[tokio::test]
async fn normal_renderer_v3_preserves_transport_and_volume_wire_contract() {
    exercise(3, false, false).await;
}
#[tokio::test]
async fn empty_optional_urls_do_not_change_valid_control_actions() {
    exercise(1, true, false).await;
}

#[tokio::test]
async fn relative_urls_are_normalized_to_root_relative_control_paths() {
    exercise(1, false, true).await;
}
