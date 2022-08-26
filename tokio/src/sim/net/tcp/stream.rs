use super::super::{addr::*, Result, IOContext, IOIntent, TcpConnectMessage, IOInterest};
use std::net::SocketAddr;


/// A TCP Stream.
#[derive(Debug)]
pub struct TcpStream {
    pub(crate) local_addr: SocketAddr,
    pub(crate) peer_addr: SocketAddr,
}

impl TcpStream {
    /// Connects
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<TcpStream> {
        let mut addrs = to_socket_addrs(addr).await?;
        let peer = addrs.next().unwrap();

        let this = IOContext::with_current(|ctx| {
            ctx.tcp_bind_stream(peer)
        })?;

        loop {
            // Initiate connect by sending a message (better repeat)
            IOContext::with_current(|ctx| {
                ctx.intents.push(IOIntent::TcpConnect(TcpConnectMessage::ClientInitiate {
                    client: this.local_addr,
                    server: this.peer_addr
                }));
            });

            let interest = IOInterest::TcpConnect((this.local_addr, this.peer_addr));
            interest.await;

            let acked = IOContext::with_current(|ctx| { 
                if let Some(handle) = ctx.tcp_streams.get(&(this.local_addr, this.peer_addr)) {
                    handle.acked
                } else {
                    panic!("Theif")
                }
            });

            if acked {
                return Ok(this)
            }
        }
    }
}