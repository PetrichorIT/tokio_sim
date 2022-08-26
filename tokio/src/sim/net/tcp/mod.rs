use std::net::SocketAddr;
use std::time::Duration;

mod listener;
pub use listener::*;

mod stream;
pub use stream::*;

#[derive(Debug, Clone)]
#[allow(unused)]
pub(super) struct TcpSocketConfig {
    addr: SocketAddr,
    linger: Option<Duration>,

    listen_backlog: u32,
    recv_buffer_size: u32,
    send_buffer_size: u32,
    reuseaddr: bool,
    reuseport: bool,

    ttl: u32,
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

            ttl: 64,
        }
    }
}
