use std::net::SocketAddr;
use std::time::Duration;

mod listener;
pub use listener::*;

mod stream;
pub use stream::*;

#[derive(Debug, Clone)]
#[allow(unused)]
pub(super) struct TcpSocketConfig {
    pub(super) addr: SocketAddr,
    pub(super) linger: Option<Duration>,

    pub(super) listen_backlog: u32,
    pub(super) recv_buffer_size: u32,
    pub(super) send_buffer_size: u32,
    pub(super) reuseaddr: bool,
    pub(super) reuseport: bool,

    pub(super) connect_timeout: Duration,
    pub(super) nodelay: bool,

    pub(super) ttl: u32,
}

impl TcpSocketConfig {
    pub(super) fn listener(addr: SocketAddr) -> TcpSocketConfig {
        TcpSocketConfig {
            addr,
            linger: None,

            listen_backlog: 32,
            recv_buffer_size: 2048,
            send_buffer_size: 2048,
            reuseaddr: true,
            reuseport: true,

            connect_timeout: Duration::from_secs(2),
            nodelay: true,

            ttl: 64,
        }
    }

    pub(super) fn stream(addr: SocketAddr) -> TcpSocketConfig {
        TcpSocketConfig {
            addr,
            linger: None,

            listen_backlog: 1,
            recv_buffer_size: 2048,
            send_buffer_size: 2048,
            reuseaddr: false,
            reuseport: false,

            connect_timeout: Duration::from_secs(2),
            nodelay: true,

            ttl: 64,
        }
    }

    pub(super) fn accept(&self, con: super::TcpListenerPendingConnection) -> TcpSocketConfig {
        TcpSocketConfig {
            addr: con.local_addr,
            linger: None,

            listen_backlog: 0,
            recv_buffer_size: 2048,
            send_buffer_size: 2048,
            reuseaddr: false,
            reuseport: false,

            connect_timeout: Duration::from_secs(2),
            nodelay: true,

            ttl: 64,
        }
    }
}
