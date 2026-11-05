use crate::kdeconnect::daemon::*;
use crate::kdeconnect::packet::PROTOCOL_VERSION;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;

/// mDNS only advertises how to reach a peer; it is not a liveness signal for an already
/// established TCP session. Android re-registers its mDNS record frequently, so give the
/// phone time to reconnect before tearing down a session that is still healthy.
const MDNS_REMOVAL_GRACE: Duration = Duration::from_secs(8);

pub(super) fn spawn_mdns(daemon: Arc<KdeConnectDaemon>, port: u16) {
    let handle = Handle::current();
    tokio::task::spawn_blocking(move || {
        daemon.run_mdns_service(port, handle);
    });
}

impl KdeConnectDaemon {
    pub(super) fn run_mdns_service(self: Arc<Self>, tcp_port: u16, handle: Handle) {
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

        let removal_generation: Arc<Mutex<HashMap<String, u64>>> =
            Arc::new(Mutex::new(HashMap::new()));

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

                            // A resolved service cancels any pending removal grace period.
                            if let Ok(mut generations) = removal_generation.lock() {
                                let token = generations.entry(txt_id.clone()).or_insert(0);
                                *token = token.wrapping_add(1);
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
                                self.spawn_discovery_attempt(
                                    IpAddr::V4(ip_val),
                                    port,
                                    txt_id.clone(),
                                    proto,
                                    None,
                                );
                            }
                        }
                        ServiceEvent::ServiceRemoved(_ty, fullname) => {
                            let removed_id = fullname.split('.').next().unwrap_or("").to_string();
                            if removed_id.is_empty() || removed_id == self.host.device_id {
                                continue;
                            }

                            let (generation, last_seen_secs) = {
                                let Ok(devices) = self.devices.lock() else {
                                    continue;
                                };
                                match devices.get(&removed_id) {
                                    Some(entry) if entry.view.reachable => {
                                        (entry.connection_generation, entry.view.last_seen_secs)
                                    }
                                    _ => continue,
                                }
                            };

                            let removal_token = {
                                let Ok(mut generations) = removal_generation.lock() else {
                                    continue;
                                };
                                let token = generations.entry(removed_id.clone()).or_insert(0);
                                *token = token.wrapping_add(1);
                                *token
                            };

                            let daemon = Arc::clone(&self);
                            let removal_generation = Arc::clone(&removal_generation);
                            handle.spawn(async move {
                                tokio::time::sleep(MDNS_REMOVAL_GRACE).await;

                                let service_came_back = removal_generation
                                    .lock()
                                    .ok()
                                    .and_then(|generations| generations.get(&removed_id).copied())
                                    != Some(removal_token);
                                if service_came_back {
                                    return;
                                }

                                let mut aborted = false;
                                if let Ok(mut devices) = daemon.devices.lock() {
                                    if let Some(entry) = devices.get_mut(&removed_id) {
                                        if entry.view.reachable
                                            && entry.connection_generation == generation
                                            && entry.view.last_seen_secs == last_seen_secs
                                        {
                                            log::info!(
                                                "[mdns] Peer '{}' ({}) did not reconnect within {:?}; closing stale session",
                                                entry.view.device_name,
                                                removed_id,
                                                MDNS_REMOVAL_GRACE
                                            );
                                            if let Some(abort_tx) = entry.abort_tx.take() {
                                                let _ = abort_tx.send(());
                                                aborted = true;
                                            }
                                        }
                                    }
                                }
                                if aborted {
                                    daemon.notify_ui();
                                }
                            });
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
