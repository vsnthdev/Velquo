use crate::kdeconnect::daemon::*;
use anyhow::{anyhow, Result};
use socket2::{SockRef, TcpKeepalive};
use std::net::Ipv4Addr;
use std::net::SocketAddrV4;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};

pub(super) async fn bind_tcp_listener() -> Result<(TcpListener, u16)> {
    let mut bound_listener = None;
    let mut chosen_port = MIN_TCP_PORT;
    for port in MIN_TCP_PORT..=MAX_TCP_PORT {
        match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await {
            Ok(listener) => {
                chosen_port = port;
                bound_listener = Some(listener);
                log::info!(
                    "[tcp-server] Listening for incoming KDE Connect TCP connections on 0.0.0.0:{}",
                    port
                );
                break;
            }
            Err(e) => {
                log::warn!(
                    "[tcp-server] Port {} unavailable ({}), trying next...",
                    port,
                    e
                );
            }
        }
    }

    let listener = bound_listener.ok_or_else(|| {
        anyhow!(
            "Could not bind any TCP port in range {}..={}",
            MIN_TCP_PORT,
            MAX_TCP_PORT
        )
    })?;
    Ok((listener, chosen_port))
}

pub(super) fn spawn_tcp_accept_loop(daemon: Arc<KdeConnectDaemon>, tcp_listener: TcpListener) {
    tokio::spawn(async move {
        loop {
            match tcp_listener.accept().await {
                Ok((stream, peer_addr)) => {
                    log::info!("[tcp-in] Accepted new TCP connection from {}", peer_addr);
                    let d = Arc::clone(&daemon);
                    tokio::spawn(async move {
                        if let Err(e) = d.handle_inbound_tcp(stream, peer_addr).await {
                            log::warn!(
                                "[tcp-in] Connection from {} closed/failed: {:?}",
                                peer_addr,
                                e
                            );
                        }
                    });
                }
                Err(e) => {
                    log::error!("[tcp-in] TCP accept error: {}", e);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    });
}

impl KdeConnectDaemon {
    /// Configures TCP_NODELAY, aggressive TCP Keepalive (2s idle, 1s interval, 2 retries),
    /// and macOS `TCP_RXT_CONNDROPTIME` (3s) so half-open Wi-Fi drops are detected rapidly.
    pub(crate) fn configure_tcp_socket(stream: &TcpStream) {
        let _ = stream.set_nodelay(true);
        let sock_ref = SockRef::from(stream);
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(2))
            .with_interval(Duration::from_secs(1))
            .with_retries(2);
        if let Err(e) = sock_ref.set_tcp_keepalive(&keepalive) {
            log::debug!("[tcp] Failed to set TCP keepalive on socket: {}", e);
        }

        #[cfg(target_os = "macos")]
        {
            const TCP_RXT_CONNDROPTIME: libc::c_int = 0x80;
            const TCP_RXT_FINDROP: libc::c_int = 0x100;
            let fd = stream.as_raw_fd();
            let drop_secs: libc::c_int = 3;
            let fin_drop: libc::c_int = 1;
            unsafe {
                let _ = libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    TCP_RXT_CONNDROPTIME,
                    &drop_secs as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
                let _ = libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    TCP_RXT_FINDROP,
                    &fin_drop as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }
    }

    /// Returns the number of unacknowledged bytes sitting in the macOS kernel TCP send buffer (`SO_NWRITE`).
    pub(crate) fn get_socket_unacked_bytes(raw_fd: RawFd) -> usize {
        #[cfg(target_os = "macos")]
        {
            let mut nwrite: libc::c_int = 0;
            let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            let res = unsafe {
                libc::getsockopt(
                    raw_fd,
                    libc::SOL_SOCKET,
                    libc::SO_NWRITE,
                    &mut nwrite as *mut _ as *mut libc::c_void,
                    &mut len,
                )
            };
            if res == 0 && nwrite > 0 {
                return nwrite as usize;
            }
            0
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = raw_fd;
            0
        }
    }
}
