use crate::kdeconnect::daemon::*;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

pub(super) fn spawn_network_monitor(daemon: Arc<KdeConnectDaemon>) {
    tokio::spawn(async move {
        // One follow-up broadcast 5s after startup once ARP/mDNS settles
        tokio::time::sleep(Duration::from_secs(5)).await;
        if daemon.should_auto_broadcast() {
            daemon.broadcast_udp_discovery().await;
        }

        let mut last_ips: Vec<Ipv4Addr> = KdeConnectDaemon::get_local_ipv4s()
            .into_iter()
            .map(|(ip, _)| ip)
            .collect();
        let mut last_host_battery: Option<(i32, bool)> =
            KdeConnectDaemon::read_macos_battery_status();
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.tick().await;
        let mut tick_count: u64 = 0;

        loop {
            interval.tick().await;
            tick_count = tick_count.wrapping_add(1);

            if tick_count.is_multiple_of(6) {
                if let Some(current_bat) = KdeConnectDaemon::read_macos_battery_status() {
                    if last_host_battery != Some(current_bat) {
                        log::info!(
                            "[battery-out] Mac battery changed ({:?} -> {:?}), broadcasting to paired devices",
                            last_host_battery,
                            current_bat
                        );
                        last_host_battery = Some(current_bat);
                        daemon.broadcast_host_battery(current_bat.0, current_bat.1);
                    }
                }
            }

            let current_ips: Vec<Ipv4Addr> = KdeConnectDaemon::get_local_ipv4s()
                .into_iter()
                .map(|(ip, _)| ip)
                .collect();

            if current_ips != last_ips {
                log::info!(
                    "[network] Local IPv4 addresses changed ({:?} -> {:?}), broadcasting discovery",
                    last_ips,
                    current_ips
                );
                last_ips = current_ips;
                daemon.notify_ui();
                daemon.broadcast_udp_discovery().await;
            } else if daemon.should_auto_broadcast() {
                log::debug!(
                    "[udp] Auto-broadcasting discovery (waiting for offline/unconnected devices)"
                );
                daemon.broadcast_udp_discovery().await;
            }
        }
    });
}
