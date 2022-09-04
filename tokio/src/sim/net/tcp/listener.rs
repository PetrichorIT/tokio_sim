use super::super::{addr::*, Result, IOContext, IOInterest};
use super::stream::TcpStream;
use std::net::SocketAddr;
use std::io::{Error, ErrorKind};
use std::task::*;

/// A TCP socket server, listening for connections.
/// 
/// You can accept a new connection by using the [accept](TcpListener::accept) method.
/// 
/// A TcpListener can be turned into a Stream with TcpListenerStream.
/// 
/// # Errors
/// 
/// Note that accepting a connection can lead to various errors and not all of them are necessarily fatal 
/// ‒ for example having too many open file descriptors or the other side closing the connection 
/// while it waits in an accept queue. These would terminate the stream if not handled in any way.
#[derive(Debug)]
pub struct TcpListener {
    pub(crate) addr: SocketAddr,
}

impl TcpListener {
    /// Creates a new TcpListener, which will be bound to the specified address.
    /// 
    /// The returned listener is ready for accepting connections.
    /// 
    /// Binding with a port number of 0 will request that the OS assigns a port to this listener. 
    /// The port allocated can be queried via the `local_addr` method.
    /// 
    /// The address type can be any implementor of the ToSocketAddrs trait. 
    /// If addr yields multiple addresses, bind will be attempted with each of the addresses 
    /// until one succeeds and returns the listener. If none of the addresses 
    /// succeed in creating a listener, the error returned from the 
    /// last attempt (the last address) is returned.
    /// 
    /// This function sets the SO_REUSEADDR option on the socket.
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> Result<TcpListener> {
        let addrs = to_socket_addrs(addr).await?;

        // Get the current context
        IOContext::with_current(|ctx| {
            let mut last_err = None;

            for addr in addrs {
                match ctx.tcp_bind_listener(addr, None) {
                    Ok(socket) => return Ok(socket),
                    Err(e) => last_err = Some(e),
                }
            }
    
            Err(last_err.unwrap_or_else(|| {
                Error::new(ErrorKind::InvalidInput, "could not resolve to any address")
            }))
        })
    }

    /// Accepts a new incoming connection from this listener.
    /// 
    /// This function will yield once a new TCP connection is established. 
    /// When established, the corresponding `TcpStream` and the remote peer’s address will be returned
    pub async fn accept(&self) -> Result<(TcpStream, SocketAddr)> {
        loop {
            let interest = IOInterest::TcpAccept(self.addr);
            interest.await?;

            let con = IOContext::with_current(|ctx| {
                ctx.tcp_accept(self.addr)
            });

            let con = match con {
                Ok(con) => con,
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        continue
                    }
                    return Err(e)
                }
            };
            let peer = con.inner.peer_addr;

            return Ok((con, peer))
        }
    }

    /// DIRTY IMPL
    pub fn poll_accept(
        &self,
        _cx: &mut Context<'_>
    ) -> Poll<Result<(TcpStream, SocketAddr)>> {
        IOContext::with_current(|ctx| {
            if let Ok(con) = ctx.tcp_accept(self.addr) {
                let peer = con.inner.peer_addr;
                Poll::Ready(Ok((con, peer)))
            } else {
                Poll::Pending
            }
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn from_std(_listener: TcpListener) -> Result<TcpListener> { unimplemented!() }

     /// DEPRECATED
     #[deprecated(note = "Not implemented in simulation context")]
    pub fn into_std(self) -> Result<TcpListener> { unimplemented!() }

    /// Returns the local address that this listener is bound to.
    /// 
    /// This can be useful, for example, when binding to port 0 to figure out which port was actually bound.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.addr)
    }

    /// Gets the value of the IP_TTL option for this socket.
    /// 
    /// For more information about this option, see [set_ttl](TcpListener::set_ttl).
    pub fn ttl(&self) -> Result<u32> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_listeners.get(&self.addr) {
                Ok(handle.config.ttl)
            } else {
                Err(Error::new(ErrorKind::Other, "Lost Tcp"))
            }
        })
    }

    /// Sets the value for the IP_TTL option on this socket.
    /// 
    /// This value sets the time-to-live field that is used in every packet sent from this socket.
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_listeners.get_mut(&self.addr) {
                handle.config.ttl = ttl;
                Ok(())
            } else {
                Err(Error::new(ErrorKind::Other, "Lost Tcp"))
            }
        })
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        IOContext::try_with_current(|ctx| ctx.tcp_drop_listener(self.addr));
    }
}