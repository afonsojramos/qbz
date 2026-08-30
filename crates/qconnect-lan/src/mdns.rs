use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use mdns_sd::{DaemonEvent, IfKind, ServiceDaemon, ServiceInfo, UnregisterStatus};

use crate::server::{LanError, SERVICE_TYPE};

const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const GOODBYE_RESEND_DELAY: Duration = Duration::from_millis(150);
const DAEMON_SHUTDOWN_RESERVE: Duration = Duration::from_millis(250);

pub(crate) struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsRegistration {
    pub(crate) fn register(
        device_uuid: &str,
        port: u16,
        sdk_version: &str,
        addresses: Option<Vec<IpAddr>>,
    ) -> Result<Self, LanError> {
        let mut addresses = match addresses {
            Some(addresses) => addresses,
            None => lan_ipv4_addresses()?,
        };
        addresses.retain(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified());
        addresses.sort_unstable();
        addresses.dedup();
        if addresses.is_empty() {
            return Err(LanError::NoLanAddresses);
        }

        let info = service_info(device_uuid, port, sdk_version, &addresses)?;
        let fullname = info.get_fullname().to_string();
        let daemon = ServiceDaemon::new()?;
        let monitor = daemon.monitor()?;
        if daemon.register(info).is_err() {
            shutdown_daemon(&daemon, Some(&fullname), SHUTDOWN_TIMEOUT);
            return Err(LanError::Mdns);
        }

        let deadline = Instant::now() + ANNOUNCE_TIMEOUT;
        let announced = loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break false;
            };
            match monitor.recv_timeout(remaining) {
                Ok(DaemonEvent::Announce(announced, _))
                    if announced.eq_ignore_ascii_case(&fullname) =>
                {
                    break true;
                }
                Ok(DaemonEvent::Error(_) | DaemonEvent::NameChange(_)) | Err(_) => break false,
                Ok(_) => {}
            }
        };
        if !announced {
            shutdown_daemon(&daemon, Some(&fullname), SHUTDOWN_TIMEOUT);
            return Err(LanError::Mdns);
        }

        Ok(Self { daemon, fullname })
    }

    pub(crate) fn shutdown(self) {
        shutdown_daemon(&self.daemon, Some(&self.fullname), SHUTDOWN_TIMEOUT);
    }
}

fn shutdown_daemon(daemon: &ServiceDaemon, fullname: Option<&str>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    if let Some(fullname) = fullname {
        if let Ok(receiver) = daemon.unregister(fullname) {
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                if matches!(receiver.recv_timeout(remaining), Ok(UnregisterStatus::OK)) {
                    // mdns-sd schedules a second goodbye 120 ms after the
                    // unregister acknowledgement. Keep the daemon alive long
                    // enough to send it, while reserving time for shutdown.
                    if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                        let goodbye_budget = remaining.saturating_sub(DAEMON_SHUTDOWN_RESERVE);
                        std::thread::sleep(GOODBYE_RESEND_DELAY.min(goodbye_budget));
                    }
                }
            }
        }
    }
    if let Ok(receiver) = daemon.shutdown() {
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let _ = receiver.recv_timeout(remaining);
        }
    }
}

fn service_info(
    device_uuid: &str,
    port: u16,
    sdk_version: &str,
    addresses: &[IpAddr],
) -> Result<ServiceInfo, mdns_sd::Error> {
    let short = short_id(device_uuid);
    let instance = format!("QBZ-{short}");
    let hostname = format!("qbz-{short}.local.");
    let properties = HashMap::from([
        ("device_uuid".to_string(), device_uuid.to_string()),
        ("sdk_version".to_string(), sdk_version.to_string()),
        ("path".to_string(), String::new()),
    ]);
    let mut info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &hostname,
        addresses,
        port,
        properties,
    )?;
    info.set_interfaces(addresses.iter().copied().map(IfKind::Addr).collect());
    Ok(info)
}

fn lan_ipv4_addresses() -> Result<Vec<IpAddr>, LanError> {
    // A connected UDP socket performs route selection without sending data.
    // Targeting the mDNS multicast group yields the IPv4 interface that can
    // actually reach LAN controllers, excluding Docker/VM bridges and
    // loopback records that make strict official clients choose a dead URL.
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
        .map_err(|_| LanError::AddressDiscovery)?;
    socket
        .connect(SocketAddr::from((Ipv4Addr::new(224, 0, 0, 251), 5353)))
        .map_err(|_| LanError::AddressDiscovery)?;
    let address = socket
        .local_addr()
        .map_err(|_| LanError::AddressDiscovery)?
        .ip();
    if !address.is_ipv4() || address.is_loopback() || address.is_unspecified() {
        return Err(LanError::NoLanAddresses);
    }
    Ok(vec![address])
}

fn short_id(device_uuid: &str) -> String {
    let short = device_uuid
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if short.is_empty() {
        "renderer".to_string()
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_short_id_is_dns_safe_and_stable() {
        assert_eq!(short_id("550e8400-e29b-41d4-a716-446655440000"), "550e8400");
        assert_eq!(short_id("---"), "renderer");
    }

    #[test]
    fn service_info_has_official_type_and_required_txt_union() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let info = service_info(uuid, 43210, "0.9.5", &["192.0.2.10".parse().unwrap()]).unwrap();

        assert_eq!(info.get_type(), SERVICE_TYPE);
        assert_eq!(info.get_port(), 43210);
        assert_eq!(info.get_property_val_str("device_uuid"), Some(uuid));
        assert_eq!(info.get_property_val_str("sdk_version"), Some("0.9.5"));
        assert_eq!(info.get_property_val_str("path"), Some(""));
        assert!(!info.is_addr_auto());
        assert_eq!(
            info.get_addresses(),
            &["192.0.2.10".parse::<IpAddr>().unwrap()]
                .into_iter()
                .collect()
        );
    }
}
