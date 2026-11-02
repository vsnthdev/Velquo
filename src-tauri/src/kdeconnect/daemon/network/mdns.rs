use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::PROTOCOL_VERSION;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::net::IpAddr;
use std::sync::Arc;

pub(super) fn spawn_mdns(daemon: Arc<KdeConnectDaemon>, port: u16) {
    tokio::task::spawn_blocking(move || {
        daemon.run_mdns_service(port);
    });
}

impl KdeConnectDaemon {
    pub(super) fn run_mdns_service(self: Arc<Self>, tcp_port: u16) {
        log::info!("[mdns] Starting mDNS daemon for _kdeconnect._udp.local.");
        let mdns = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("[mdns] Could not initialize mDNS daemon: {}", e);
                return;
            }
        };

        let service_type = "_kdeconnect._udp.local.";
        let instance_name = self.host.device_id.clone();
        let host_name = format!("{}.local.", self.host.device_id);
        let local_ips: Vec<IpAddr> = Self::get_local_ipv4s()
            .into_iter()
            .map(|(ip, _)| IpAddr::V4(ip))
            .collect();

        let properties = [
            ("id", self.host.device_id.as_str()),
            ("name", self.host.device_name.as_str()),
            ("type", self.host.device_type.as_str()),
            ("protocol", "8"),
        ];

        match ServiceInfo::new(
            service_type,
            &instance_name,
            &host_name,
            &local_ips[..],
            tcp_port,
            &properties[..],
        ) {
            Ok(service_info) => {
                if let Err(e) = mdns.register(service_info) {
                    log::warn!("[mdns] Failed to register mDNS service: {}", e);
                } else {
                    log::info!(
                        "[mdns] Registered mDNS service {} on port {} with IPs {:?}",
                        instance_name,
                        tcp_port,
                        local_ips
                    );
                }
            }
            Err(e) => {
                log::warn!("[mdns] Failed to build ServiceInfo: {}", e);
            }
        }

        match mdns.browse(service_type) {
            Ok(receiver) => {
                log::info!("[mdns] Browsing for _kdeconnect._udp.local. services...");
                while let Ok(event) = receiver.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let txt_id = info
                                .get_property_val_str("id")
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| {
                                    info.get_fullname()
                                        .split('.')
                                        .next()
                                        .unwrap_or("")
                                        .to_string()
                                });

                            if txt_id.is_empty() || txt_id == self.host.device_id {
                                continue;
                            }

                            let proto = info
                                .get_property_val_str("protocol")
                                .and_then(|p| p.parse::<i32>().ok())
                                .unwrap_or(PROTOCOL_VERSION);
                            let port = info.get_port();

                            for ip in info.get_addresses_v4() {
                                let ip_val = *ip;
                                log::info!(
                                    "[mdns] Discovered peer via mDNS: id='{}' ip={} port={} proto={}",
                                    txt_id,
                                    ip_val,
                                    port,
                                    proto
                                );
                                let d = Arc::clone(&self);
                                let id_clone = txt_id.clone();
                                tauri::async_runtime::spawn(async move {
                                    d.on_device_discovered(
                                        IpAddr::V4(ip_val),
                                        port,
                                        id_clone,
                                        proto,
                                        None,
                                    )
                                    .await;
                                });
                            }
                        }
                        ServiceEvent::ServiceRemoved(_ty, fullname) => {
                            let removed_id = fullname.split('.').next().unwrap_or("").to_string();
                            if !removed_id.is_empty() && removed_id != self.host.device_id {
                                let mut aborted = false;
                                if let Ok(mut devices) = self.devices.lock() {
                                    if let Some(entry) = devices.get_mut(&removed_id) {
                                        if entry.view.reachable {
                                            log::info!(
                                                "[mdns] Peer '{}' ({}) unregistered mDNS service, closing active session",
                                                entry.view.device_name,
                                                removed_id
                                            );
                                            if let Some(abort_tx) = entry.abort_tx.take() {
                                                let _ = abort_tx.send(());
                                                aborted = true;
                                            }
                                        }
                                    }
                                }
                                if aborted {
                                    self.notify_ui();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                log::warn!("[mdns] Failed to browse mDNS: {}", e);
            }
        }
    }
}
