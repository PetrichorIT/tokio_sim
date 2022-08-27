use crate::io::{ReadBuf, Ready};
use super::{addr::*, Result, IOContext, IOInterest, Interest};
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::task::*;
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


/// A UDO socket.
/// 
/// UDP is “connectionless”, unlike TCP. Meaning, regardless of what address you’ve bound to, 
/// a UdpSocket is free to communicate with many different remotes. 
/// In Tokio there are basically two main ways to use `UdpSocket`:
/// 
/// * one to many: [`bind`](`UdpSocket::bind`) and use [`send_to`](`UdpSocket::send_to`)
///   and [`recv_from`](`UdpSocket::recv_from`) to communicate with many different addresses
/// * one to one: [`connect`](`UdpSocket::connect`) and associate with a single address, using [`send`](`UdpSocket::send`)
///   and [`recv`](`UdpSocket::recv`) to communicate only with that remote address
///
/// This type does not provide a `split` method, because this functionality
/// can be achieved by instead wrapping the socket in an [`Arc`]. Note that
/// you do not need a `Mutex` to share the `UdpSocket` — an `Arc<UdpSocket>`
/// is enough. This is because all of the methods take `&self` instead of
/// `&mut self`. Once you have wrapped it in an `Arc`, you can call
/// `.clone()` on the `Arc<UdpSocket>` to get multiple shared handles to the
/// same socket. An example of such usage can be found further down.
#[derive(Debug)]
pub struct UdpSocket {
    pub(super) addr: SocketAddr,
}

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to the `addr` provided.
    /// 
    /// Binding with a port number of 0 will request that the OS assigns a port to this listener. 
    /// The port allocated can be queried via the `local_addr` method.
    pub async fn bind(addr: impl ToSocketAddrs) -> Result<UdpSocket> {
        let addrs = to_socket_addrs(addr).await?;
      
        // Get the current context
        IOContext::with_current(|ctx| {
            let mut last_err = None;

            for addr in addrs {
                match ctx.udp_bind(addr) {
                    Ok(socket) => return Ok(socket),
                    Err(e) => last_err = Some(e),
                }
            }
    
            Err(last_err.unwrap_or_else(|| {
                Error::new(ErrorKind::InvalidInput, "could not resolve to any address")
            }))
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn from_std(socket: std::net::UdpSocket) -> Result<UdpSocket> {
        panic!("No implemented for feature 'sim'")
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot extract std::net::UdpSocket from simulated socket")]
    pub fn into_std(self) -> Result<std::net::UdpSocket> {
        panic!("No implemented for feature 'sim'")
    }

    /// Returns the local address that this socket is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.addr)
    }

    /// Returns the socket address of the remote peer this socket was connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        IOContext::with_current(|ctx| 
            if let Some(peer) = ctx.udp_peer(self.addr) {
                Ok(peer)
            } else {
                return Err(Error::new(ErrorKind::Other, "No Peer"))
            }
        )
    }

    /// Connects the UDP socket setting the default destination for send() and 
    /// limiting packets that are read via recv from the address specified in `addr`.
    pub async fn connect<A: ToSocketAddrs>(&self, addr: A) -> Result<()> {
        let addrs = to_socket_addrs(addr).await?;

        IOContext::with_current(|ctx| {
            let mut last_err = None;
            for peer in addrs {
                match ctx.udp_connect(self.addr, peer) {
                    Ok(()) => {
                        return Ok(())
                    },
                    Err(e) => last_err = Some(e)
                }
            }

            Err(last_err.unwrap())
        })
    }

    /// Waits for any of the requested ready states.
    /// 
    /// This function is usually paired with `try_recv()` or `try_send()`. 
    /// It can be used to concurrently recv / send to the same socket on a single task without 
    /// splitting the socket.
    /// 
    /// The function may complete without the socket being ready. 
    /// This is a false-positive and attempting an operation will return with `io::ErrorKind::WouldBlock`.
    pub async fn ready(&self, interest: Interest) -> Result<Ready> {
        let (io, ready) = interest.udp_io_interest(self.addr);
        io.await?;
        Ok(ready)
    }

    /// Waits for the socket to become writable.
    /// 
    /// This function is equivalent to `ready(Interest::WRITABLE)` and is usually 
    /// paired with `try_send()` or `try_send_to()`.
    /// 
    /// The function may complete without the socket being writable. 
    /// This is a false-positive and attempting a `try_send()` will return with `io::ErrorKind::WouldBlock`.
    pub async fn writable(&self) -> Result<()> {
        self.ready(Interest::WRITABLE).await?;
        Ok(())
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_send_ready(&self, cx: &mut Context<'_>) -> Poll<Result<()>> {
        unimplemented!()
    }

    /// Sends data on the socket to the remote address that the socket is connected to.
    /// 
    /// The [connect] method will connect this socket to a remote address. 
    /// This method will fail if the socket is not connected.
    pub async fn send(&self, buf: &[u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            let peer = if let Some(peer) = ctx.udp_peer(self.addr) {
                peer
            } else {
                return Err(Error::new(ErrorKind::Other, "No Peer"))
            };

            ctx.udp_send(self.addr, peer, Vec::from(buf))?;
            Ok(buf.len())
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_send(&self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>> {
        unimplemented!()
    }

    /// Tries to send data on the socket to the remote address to which it is connected.
    /// 
    /// When the socket buffer is full, Err(io::ErrorKind::WouldBlock) is returned. 
    /// This function is usually paired with writable().
    pub fn try_send(&self, buf: &[u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            let peer = if let Some(peer) = ctx.udp_peer(self.addr) {
                peer
            } else {
                return Err(Error::new(ErrorKind::Other, "No Peer"))
            };

            ctx.udp_send(self.addr, peer, Vec::from(buf))?;
            Ok(buf.len())
        })
    }

    /// Waits for the socket to become readable.
    /// 
    /// This function is equivalent to `ready(Interest::READABLE)` and is usually paired with `try_recv()`.
    /// 
    /// The function may complete without the socket being readable. 
    /// This is a false-positive and attempting a `try_recv()` will return with `io::ErrorKind::WouldBlock`.
    pub async fn readable(&self) -> Result<()> {
        self.ready(Interest::READABLE).await?;
        Ok(())
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_recv_ready(&self, cx: &mut Context<'_>) -> Poll<Result<()>> {
        unimplemented!();
    }

    /// Receives a single datagram message on the socket from the remote address to 
    /// which it is connected. On success, returns the number of bytes read.
    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let peer = IOContext::with_current(|ctx| 
            if let Some(peer) = ctx.udp_peer(self.addr) {
                Ok(peer)
            } else {
                return Err(Error::new(ErrorKind::Other, "No Peer"))
            }
        )?;

        loop {
            let interest = IOInterest::UdpRead(self.addr);
            interest.await?;

            let r = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx
                    .udp_sockets
                    .get_mut(&self.addr)   
                {
                    handle.incoming.pop_front()
                } else {
                    panic!("SimContext lost socket")
                }
            });
           
            match r {
                Some(msg) => {
                    if msg.src_addr != peer {
                        continue;
                    }

                    let wrt = msg.content.len().min(buf.len());
                    for i in 0..wrt {
                        buf[i] = msg.content[i];
                    }

                    return Ok(wrt);
                }
                None => {}
            }
        }
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<Result<()>> {
        unimplemented!()
    }

    /// Tries to receive a single datagram message on the socket from the remote address to which it is connected. 
    /// On success, returns the number of bytes read.
    /// 
    /// The function must be called with valid byte array buf of sufficient size to hold the message bytes. 
    /// If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
    pub fn try_recv(&self, buf: &mut [u8]) -> Result<usize> {
        loop { 
            let (peer, r) = IOContext::with_current(|ctx| {
                let peer = if let Some(peer) = ctx.udp_peer(self.addr) {
                    peer
                } else {
                    return Err(Error::new(ErrorKind::Other, "No Peer"))
                };

                if let Some(handle) = ctx
                    .udp_sockets
                    .get_mut(&self.addr)   
                {
                    Ok((peer, handle.incoming.pop_front()))
                } else {
                    panic!("SimContext lost socket")
                }
            })?;
            
            match r {
                Some(msg) => {
                    if msg.src_addr != peer {
                        continue;
                    }

                    let wrt = msg.content.len().min(buf.len());
                    for i in 0..wrt {
                        buf[i] = msg.content[i];
                    }

                    return Ok(wrt);
                }
                None => {
                    return Err(Error::new(ErrorKind::WouldBlock, "Would block"))
                }
            }
        }
    }

    /// Sends data on the socket to the given address. On success, returns the number of bytes written.
    /// 
    /// Address type can be any implementor of [ToSocketAddrs] trait. See its documentation for concrete examples.
    ///
    /// It is possible for `addr` to yield multiple addresses, 
    /// but `send_to` will only send data to the first address yielded by `addr`.
    pub async fn send_to(&self, buf: &[u8], target: impl ToSocketAddrs) -> Result<usize> {
        let addr = to_socket_addrs(target).await;
        let first = addr.unwrap().next().unwrap();

        IOContext::with_current(|ctx| {
            ctx.udp_send(self.addr, first, Vec::from(buf))
        })?;

        Ok(buf.len())
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_send_to(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr
    ) -> Poll<Result<usize>> {
        unimplemented!()
    }

    /// Tries to send data on the socket to the given address, 
    /// but if the send is blocked this will return right away.
    /// 
    /// This function is usually paired with writable().
    pub fn try_send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize> {
        IOContext::with_current(|ctx| {
            ctx.udp_send(self.addr, target, Vec::from(buf))
        })?;

        Ok(buf.len())
    }

    /// Receives a single datagram message on the socket. On success, 
    /// returns the number of bytes read and the origin.
    /// 
    /// The function must be called with valid byte array buf of sufficient size to hold the message bytes. 
    /// If a message is too long to fit in the supplied buffer, excess bytes may be discarded.
    pub async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        loop {
            let interest = IOInterest::UdpRead(self.addr);
            interest.await?;

            let r = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx
                    .udp_sockets
                    .get_mut(&self.addr)   
                {
                    handle.incoming.pop_front()
                } else {
                    panic!("SimContext lost socket")
                }
            });

            match r {
                Some(msg) => {
                    let wrt = msg.content.len().min(buf.len());
                    for i in 0..wrt {
                        buf[i] = msg.content[i];
                    }

                    return Ok((wrt, msg.src_addr));
                }
                None => {return Err(Error::new(ErrorKind::WouldBlock, "Would block"))}
            }
        }
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn poll_recv_from(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<Result<SocketAddr>> {
        unimplemented!()
    }

    /// Tries to receive a single datagram message on the socket. 
    /// On success, returns the number of bytes read and the origin.
    /// 
    /// The function must be called with valid byte array buf of sufficient size 
    /// to hold the message bytes. If a message is too long to fit in the supplied buffer, 
    /// excess bytes may be discarded.
    pub fn try_recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        loop {
            let r = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx
                    .udp_sockets
                    .get_mut(&self.addr)   
                {
                    handle.incoming.pop_front()
                } else {
                    panic!("SimContext lost socket")
                }
            });

            match r {
                Some(msg) => {
                    let wrt = msg.content.len().min(buf.len());
                    for i in 0..wrt {
                        buf[i] = msg.content[i];
                    }

                    return Ok((wrt, msg.src_addr));
                }
                None => {}
            }
        }
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn try_io<R>(
        &self,
        interest: Interest,
        f: impl FnOnce() -> Result<R>
    ) -> Result<R> {
        unimplemented!()
    }

    /// Gets the value of the `SO_BROADCAST option for this socket.
    /// 
    /// For more information about this option, see [set_broadcast]
    pub fn broadcast(&self) -> Result<bool> {
        IOContext::with_current(|ctx| {
            match ctx.udp_sockets.get(&self.addr) {
                Some(ref sock) => Ok(sock.broadcast),
                None => Err(Error::new(ErrorKind::Other, "SimContext lost socket handle"))
            }
        })
    }
    
    /// Sets the value of the SO_BROADCAST option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast address.
    pub fn set_broadcast(&self, on: bool) -> Result<()> {
        IOContext::with_current(|ctx| {
            match ctx.udp_sockets.get_mut(&self.addr) {
                Some(sock) => {
                    sock.broadcast = on;
                    Ok(())
                },
                None => Err(Error::new(ErrorKind::Other, "SimContext lost socket handle"))
            }
        })
    }

    /// Gets the value of the IP_TTL option for this socket.
    ///
    /// For more information about this option, see [set_ttl].
    /// 
    pub fn ttl(&self) -> Result<u32> {
        IOContext::with_current(|ctx| {
            match ctx.udp_sockets.get(&self.addr) {
                Some(ref sock) => Ok(sock.ttl),
                None => Err(Error::new(ErrorKind::Other, "SimContext lost socket handle"))
            }
        })
    }

    /// Sets the value for the IP_TTL option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent from this socket.
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        IOContext::with_current(|ctx| {
            match ctx.udp_sockets.get_mut(&self.addr) {
                Some(sock) => {
                    sock.ttl = ttl;
                    Ok(())
                },
                None => Err(Error::new(ErrorKind::Other, "SimContext lost socket handle"))
            }
        })
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        IOContext::try_with_current(|ctx| {
            ctx.udp_drop(self.addr)
        });
    }
}