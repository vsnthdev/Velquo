use crate::kdeconnect::daemon::*;
use anyhow::Result;
use std::sync::Arc;

mod mdns;
mod monitor;
mod tcp;
mod udp;

impl KdeConnectDaemon {
    /// Starts TCP server, UDP discovery listener + periodic broadcaster, and mDNS service
    pub fn start(self: &Arc<Self>) {
        let daemon = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = daemon.run_network_services().await {
                log::error!("[daemon] Fatal error starting network services: {:?}", e);
            }
        });
    }

    async fn run_network_services(self: Arc<Self>) -> Result<()> {
        let (tcp_listener, chosen_port) = tcp::bind_tcp_listener().await?;
        *self.tcp_port.lock().unwrap() = chosen_port;
        self.notify_ui();

        let udp_sock = udp::bind_udp_discovery_socket()?;
        let udp_arc = Arc::new(udp_sock);
        *self.udp_socket.lock().unwrap() = Some(Arc::clone(&udp_arc));
        log::info!(
            "[udp] Bound UDP socket on 0.0.0.0:{} (broadcast=true, reuse_addr=true, reuse_port=true)",
            UDP_PORT
        );

        tcp::spawn_tcp_accept_loop(Arc::clone(&self), tcp_listener);
        udp::spawn_udp_receive_loop(Arc::clone(&self), udp_arc);
        mdns::spawn_mdns(Arc::clone(&self), chosen_port);

        self.broadcast_udp_discovery().await;
        monitor::spawn_network_monitor(Arc::clone(&self));
        super::clipboard::spawn_clipboard_monitor(Arc::clone(&self));
        super::mpris::control::spawn_media_broadcaster(Arc::clone(&self));

        Ok(())
    }

    /// Returns true ONLY if at least one already-paired (trusted) device is currently offline.
    /// Searching for new unpaired devices only happens when the user opens the "Pair new device" page.
    pub(super) fn should_auto_broadcast(&self) -> bool {
        let devices = self.devices.lock().unwrap();
        devices
            .values()
            .any(|d| d.view.pair_state == PairState::Paired && !d.view.reachable)
    }
}
