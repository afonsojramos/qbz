//! Regression gate for #745 — DLNA renderers vanished in 2.1.0.
//!
//! A report of a missing KEF LSX (gen 1) included discovery failures with
//! `Invalid response: empty string`; the failing device XML was not attached.
//! A reproduced cause consistent with that error: `rupnp` parses service
//! URLs as `http::uri::PathAndQuery`, and `http` 1.5.0 started
//! rejecting inputs that 1.4.0 accepted — an empty element, and any relative
//! URL without a leading slash. These fixtures are synthetic, not captured KEF XML.
//!
//! The tolerance cases fail with upstream rupnp + http 1.5.0. The local
//! rupnp patch restores them while keeping http 1.5.0 for the whole workspace.

use std::net::SocketAddr;
use std::thread::JoinHandle;

/// Renderer description whose services carry EMPTY sub-URLs.
const EMPTY_SERVICE_URLS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>KEF LSX</friendlyName>
    <manufacturer>KEF</manufacturer>
    <modelName>LSX</modelName>
    <UDN>uuid:00000000-0000-0000-0000-000000000745</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <SCPDURL></SCPDURL>
        <controlURL>/AVTransport/control</controlURL>
        <eventSubURL>/AVTransport/event</eventSubURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <SCPDURL></SCPDURL>
        <controlURL>/RenderingControl/control</controlURL>
        <eventSubURL>/RenderingControl/event</eventSubURL>
      </service>
    </serviceList>
  </device>
</root>
"#;

/// Renderer description whose services carry RELATIVE sub-URLs (no leading
/// slash) — the other shape `http` 1.5.0 turned into a hard error.
const RELATIVE_SERVICE_URLS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>Legacy Renderer</friendlyName>
    <manufacturer>Acme</manufacturer>
    <modelName>Streamer</modelName>
    <UDN>uuid:00000000-0000-0000-0000-000000000733</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
        <SCPDURL>MediaRenderer/AVTransport/desc.xml</SCPDURL>
        <controlURL>MediaRenderer/AVTransport/control</controlURL>
        <eventSubURL>MediaRenderer/AVTransport/event</eventSubURL>
      </service>
      <service>
        <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
        <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
        <SCPDURL>MediaRenderer/RenderingControl/desc.xml</SCPDURL>
        <controlURL>MediaRenderer/RenderingControl/control</controlURL>
        <eventSubURL>MediaRenderer/RenderingControl/event</eventSubURL>
      </service>
    </serviceList>
  </device>
</root>
"#;

/// Serve `body` once on an ephemeral loopback port, like the renderer would.
fn serve_once(body: &'static str) -> (SocketAddr, JoinHandle<()>) {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind loopback");
    let addr = server
        .server_addr()
        .to_ip()
        .expect("loopback listener has an ip");

    let handle = std::thread::spawn(move || {
        if let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
            let response = tiny_http::Response::from_string(body).with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/xml"[..])
                    .expect("static header"),
            );
            let _ = request.respond(response);
        }
    });

    (addr, handle)
}

async fn parse_description(body: &'static str) -> rupnp::Device {
    let (addr, handle) = serve_once(body);
    let url: http::Uri = format!("http://{addr}/description.xml")
        .parse()
        .expect("loopback url");

    let device = rupnp::Device::from_url(url)
        .await
        .expect("a MediaRenderer description must parse");

    handle.join().expect("description server thread");
    device
}

fn has_service(device: &rupnp::Device, needle: &str) -> bool {
    device
        .services_iter()
        .any(|service| service.service_type().to_string().contains(needle))
}

#[tokio::test]
async fn description_with_empty_service_urls_still_parses() {
    let device = parse_description(EMPTY_SERVICE_URLS).await;

    assert_eq!(device.friendly_name(), "KEF LSX");
    assert!(has_service(&device, ":service:AVTransport:"));
    assert!(has_service(&device, ":service:RenderingControl:"));
}

#[tokio::test]
async fn description_with_relative_service_urls_still_parses() {
    let device = parse_description(RELATIVE_SERVICE_URLS).await;

    assert_eq!(device.friendly_name(), "Legacy Renderer");
    assert!(has_service(&device, ":service:AVTransport:"));
    assert!(has_service(&device, ":service:RenderingControl:"));
}

#[tokio::test]
async fn empty_http_body_is_an_xml_error_not_empty_uri() {
    let (addr, handle) = serve_once("");
    let result =
        rupnp::Device::from_url(format!("http://{addr}/description.xml").parse().unwrap()).await;
    handle.join().unwrap();
    assert!(
        matches!(result, Err(rupnp::Error::XmlError(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn malformed_and_oversized_service_urls_remain_rejected() {
    for invalid in [
        "bad path".to_string(),
        "/bad path".to_string(),
        "x".repeat(70_000),
    ] {
        let body: &'static str = Box::leak(
            EMPTY_SERVICE_URLS
                .replace("/AVTransport/control", &invalid)
                .into_boxed_str(),
        );
        let (addr, handle) = serve_once(body);
        let result =
            rupnp::Device::from_url(format!("http://{addr}/description.xml").parse().unwrap())
                .await;
        handle.join().unwrap();
        assert!(
            matches!(result, Err(rupnp::Error::InvalidResponse(_))),
            "{result:?}"
        );
    }
}
