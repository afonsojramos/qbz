use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::server::{LanError, SERVICE_TYPE};

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
        daemon.register(info)?;

        Ok(Self { daemon, fullname })
    }

    pub(crate) fn shutdown(self) {
        if let Ok(receiver) = self.daemon.unregister(&self.fullname) {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
        if let Ok(receiver) = self.daemon.shutdown() {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
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
    ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &hostname,
        addresses,
        port,
        properties,
    )
    .map(ServiceInfo::enable_addr_auto)
}

fn lan_ipv4_addresses() -> Result<Vec<IpAddr>, LanError> {
    let mut addresses = if_addrs::get_if_addrs()
        .map_err(|_| LanError::AddressDiscovery)?
        .into_iter()
        .map(|interface| interface.ip())
        .filter(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified())
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    Ok(addresses)
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
    }
}
