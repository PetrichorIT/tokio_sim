use super::{addr::*, Result, InterfaceStatus};
use crate::sim::ctx::*;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

/*
The Tasks that mus be handled:

UdpSocket::bind (directly using interfaces no async)
UpdSocket::connect (directly using interfaces no asnyc)
UdpSocket::ready -- Interest q
    - readable()
UpdSocket::send (direct resolve)
    - send_to
UpdSOcket::recv (Mark interest, search buffers.)
    - recv_from
    - peek_from
*/


/// A udp socket
#[derive(Debug)]
pub struct UdpSocket {
    addr: SocketAddr,
}

impl UdpSocket {
    /// Binds
    pub async fn bind(addr: impl ToSocketAddrs) -> Result<Self> {
        let addrs = to_socket_addrs(addr).await?;
        let mut last_err = None;

        for addr in addrs {
            match UdpSocket::bind_addr(addr) {
                Ok(socket) => return Ok(socket),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            Error::new(ErrorKind::InvalidInput, "could not resolve to any address")
        }))
    }

    fn bind_addr(addr: SocketAddr) -> Result<Self> {
        IOContext::with_current(|ctx| {
            // (1) check loopback;
            if addr.ip().is_loopback() {
                if let Some(lo0) = ctx.interfaces.iter().find(|i| i.flags.loopback) {
                    // Find buffer
                    if ctx.udp_sockets.iter().any(|s| *s.state.addr() == addr) {
                        return Err(Error::new(ErrorKind::AddrInUse, "Address allready in use"));
                    }

                    // Check status
                    if lo0.status == InterfaceStatus::Inactive {
                        return Err(Error::new(ErrorKind::NotFound, "Interface inactive"));
                    }

                    if !lo0.flags.up {
                        return Err(Error::new(ErrorKind::NotFound, "Interface down"));
                    }

                    // TODO: Check addr
                    let buf = UdpSocketBuffer {
                        state: UdpSocketState::Bound(addr),
                        incoming: Vec::with_capacity(8),
                        outgoing: Vec::with_capacity(8),
                    };

                    ctx.udp_sockets.push(buf);

                    return Ok(UdpSocket { addr });
                }
            }

            Err(Error::new(ErrorKind::Other, "TODO"))
        })
    }

    /// Send
    pub async fn send_to(&self, buf: &[u8], target: impl ToSocketAddrs) -> Result<usize> {
        let addr = to_socket_addrs(target).await;
        let first = addr.unwrap().next().unwrap();

        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx
                .udp_sockets
                .iter_mut()
                .find(|s| *s.state.addr() == self.addr)
            {
                handle.outgoing.push(UdpMessage {
                    from: self.addr,
                    to: first,
                    buffer: Vec::from(buf),
                })
            }
        });
        Ok(buf.len())
    }

    /// Recv From
    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        loop {
            let interest = IOInterest { addr: self.addr };
            interest.await;

            let r = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx
                    .udp_sockets
                    .iter_mut()
                    .find(|s| *s.state.addr() == self.addr)
                {
                    handle.incoming.pop()
                } else {
                    panic!("HÄH")
                }
            });

            match r {
                Some(msg) => {
                    let wrt = msg.buffer.len().min(buf.len());
                    for i in 0..wrt {
                        buf[i] = msg.buffer[i];
                    }

                    return Ok((wrt, msg.from));
                }
                None => {}
            }
        }
    }
}

