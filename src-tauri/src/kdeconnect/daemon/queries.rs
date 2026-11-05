use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::*;
use anyhow::{anyhow, Result};
use rustls::pki_types::CertificateDer;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::Ordering;

impl KdeConnectDaemon {
    pub fn get_local_ipv4s() -> Vec<(Ipv4Addr, Ipv4Addr)> {
        let mut list = Vec::new();
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for iface in interfaces {
                if iface.is_loopback() {
                    continue;
                }
                if let if_addrs::IfAddr::V4(v4) = iface.addr {
                    let ip = v4.ip;
                    let netmask = v4.netmask;
                    let broadcast = v4
                        .broadcast
                        .unwrap_or_else(|| Ipv4Addr::from(u32::from(ip) | !u32::from(netmask)));
                    list.push((ip, broadcast));
                }
            }
        }
        list
    }

    pub fn get_host_info(&self) -> HostInfo {
        let tcp_port = *self.tcp_port.lock().unwrap();
        HostInfo {
            device_id: self.host.device_id.clone(),
            device_name: self.host.device_name.clone(),
            device_type: self.host.device_type.clone(),
            protocol_version: PROTOCOL_VERSION,
            tcp_port,
            udp_port: UDP_PORT,
            cert_fingerprint: self.host.cert_fingerprint.clone(),
            downloads_dir: self.downloads_dir.to_string_lossy().to_string(),
            log_file_path: self.log_file_path.to_string_lossy().to_string(),
            ignore_whatsapp_conversations: self.is_ignore_whatsapp_conversations_enabled(),
        }
    }

    pub fn get_devices_list(&self) -> Vec<DeviceView> {
        let guard = self.devices.lock().unwrap();
        let mut list: Vec<DeviceView> = guard.values().map(|e| e.view.clone()).collect();
        list.sort_by(|a, b| {
            b.reachable
                .cmp(&a.reachable)
                .then_with(|| a.device_name.cmp(&b.device_name))
        });
        list
    }

    pub fn get_transfers_list(&self) -> Vec<TransferItem> {
        let guard = self.transfers.lock().unwrap();
        let mut list = guard.clone();
        list.sort_by_key(|b| std::cmp::Reverse(b.timestamp_ms));
        list
    }

    /// Starts TCP server, UDP discovery listener + periodic broadcaster, and mDNS service
    pub fn send_packet_to_device(&self, device_id: &str, packet: &NetworkPacket) -> Result<()> {
        let bytes = packet.serialize_line()?;
        let devices = self.devices.lock().unwrap();
        let entry = devices
            .get(device_id)
            .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
        let sender = entry
            .sender
            .as_ref()
            .ok_or_else(|| anyhow!("Device '{}' is currently offline", entry.view.device_name))?;
        sender
            .send(bytes)
            .map_err(|_| anyhow!("Failed to send packet to device channel"))?;
        if packet.packet_type != PACKET_TYPE_MOUSEPAD_REQUEST {
            log::info!(
                "[packet-out] Sent '{}' to '{}' ({})",
                packet.packet_type,
                entry.view.device_name,
                device_id
            );
        }
        Ok(())
    }

    pub fn is_device_notifications_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.sync_notifications)
            .unwrap_or(false)
    }

    pub fn is_device_auto_copy_otp_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.auto_copy_otp)
            .unwrap_or(false)
    }

    pub fn is_device_pause_media_on_calls_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.pause_media_on_calls)
            .unwrap_or(false)
    }

    pub fn is_device_paired(&self, device_id: &str) -> bool {
        self.devices
            .lock()
            .ok()
            .and_then(|devices| {
                devices
                    .get(device_id)
                    .map(|entry| entry.paired.load(Ordering::Acquire))
            })
            .unwrap_or(false)
    }

    pub fn is_device_presenter_allowed(&self, device_id: &str) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .get(device_id)
            .map(|d| d.view.pair_state == PairState::Paired && d.view.allow_presenter)
            .unwrap_or(false)
    }

    pub fn has_online_clipboard_sync_device(&self) -> bool {
        let devices = self.devices.lock().unwrap();
        devices.values().any(|d| {
            d.view.reachable && d.view.pair_state == PairState::Paired && d.view.sync_clipboard
        })
    }

    pub(super) fn payload_peer_identity(&self, device_id: &str) -> Result<(IpAddr, Vec<u8>)> {
        let devices = self.devices.lock().unwrap();
        let entry = devices
            .get(device_id)
            .ok_or_else(|| anyhow!("Device {} not found", device_id))?;
        if !entry.paired.load(Ordering::Acquire) {
            return Err(anyhow!("Device {} is not paired", device_id));
        }
        if !entry.view.reachable {
            return Err(anyhow!("Device {} is offline", device_id));
        }
        let expected_ip = entry
            .view
            .address
            .parse::<IpAddr>()
            .map_err(|_| anyhow!("Device {} has no valid payload address", device_id))?;
        let expected_cert = entry
            .peer_cert_der
            .clone()
            .ok_or_else(|| anyhow!("Device {} has no pinned certificate", device_id))?;
        Ok((expected_ip, expected_cert))
    }

    pub(super) fn verify_pinned_payload_cert(
        device_id: &str,
        peer_cert: Option<&CertificateDer<'_>>,
        expected_cert_der: &[u8],
    ) -> Result<()> {
        let peer_cert = peer_cert.ok_or_else(|| {
            anyhow!(
                "Payload peer for device {} presented no TLS certificate",
                device_id
            )
        })?;
        if peer_cert.as_ref() != expected_cert_der {
            return Err(anyhow!(
                "Payload TLS certificate mismatch for device {}",
                device_id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_payload_certificate() {
        let cert = CertificateDer::from(vec![1, 2, 3, 4]);
        assert!(
            KdeConnectDaemon::verify_pinned_payload_cert("device", Some(&cert), &[1, 2, 3, 4])
                .is_ok()
        );
    }

    #[test]
    fn rejects_mismatched_payload_certificate() {
        let cert = CertificateDer::from(vec![1, 2, 3, 4]);
        assert!(
            KdeConnectDaemon::verify_pinned_payload_cert("device", Some(&cert), &[4, 3, 2, 1])
                .is_err()
        );
    }

    #[test]
    fn rejects_missing_payload_certificate() {
        assert!(
            KdeConnectDaemon::verify_pinned_payload_cert("device", None, &[1, 2, 3, 4]).is_err()
        );
    }
}
