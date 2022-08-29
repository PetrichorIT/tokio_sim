use super::super::{IOContext, IOInterest, Result, TcpStream, TcpListener};
use super::TcpSocketConfig;

use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::time::Duration;

/// A TCP socket that has not yet been converted to a TcpStream or TcpListener.
///
/// TcpSocket wraps an operating system socket and enables the caller to configure
/// the socket before establishing a TCP connection or accepting inbound connections.
/// The caller is able to set socket option and explicitly
/// bind the socket with a socket address.
///
/// The underlying socket is closed when the TcpSocket value is dropped.
///
/// TcpSocket should only be used directly if the default configuration
/// used by TcpStream::connect and TcpListener::bind does not meet the required use case.
#[derive(Debug)]
pub struct TcpSocket {
    pub(super) config: RefCell<TcpSocketConfig>,
    pub(crate) expect_v4: bool,
}

impl TcpSocket {
    /// Creates a new socket configured for IPv4.
    pub fn new_v4() -> Result<TcpSocket> {
        Ok(TcpSocket {
            config: RefCell::new(TcpSocketConfig::socket()),
            expect_v4: true,
        })
    }

    /// Creates a new socket configured for IPv6.
    pub fn new_v6() -> Result<TcpSocket> {
        Ok(TcpSocket {
            config: RefCell::new(TcpSocketConfig::socket()),
            expect_v4: false,
        })
    }

    /// Allows the socket to bind to an in-use address.
    ///
    /// Behavior is platform specific.
    /// Refer to the target platform’s documentation for more details.
    pub fn set_reuseaddr(&self, reuseaddr: bool) -> Result<()> {
        self.config.borrow_mut().reuseaddr = reuseaddr;
        Ok(())
    }

    /// Retrieves the value set for SO_REUSEADDR on this socket.
    pub fn reuseaddr(&self) -> Result<bool> {
        Ok(self.config.borrow().reuseaddr)
    }

    /// Allows the socket to bind to an in-use port.
    /// Only available for unix systems (excluding Solaris & Illumos).
    ///
    /// Behavior is platform specific. Refer to the target platform’s documentation for more details.
    pub fn set_reuseport(&self, reuseport: bool) -> Result<()> {
        self.config.borrow_mut().reuseport = reuseport;
        Ok(())
    }

    /// Allows the socket to bind to an in-use port.
    /// Only available for unix systems (excluding Solaris & Illumos).
    ///
    /// Behavior is platform specific. Refer to the target platform’s documentation for more details.
    pub fn reuseport(&self) -> Result<bool> {
        Ok(self.config.borrow().reuseport)
    }

    /// Sets the size of the TCP send buffer on this socket.
    ///
    /// On most operating systems, this sets the SO_SNDBUF socket option.
    pub fn set_send_buffer_size(&self, size: u32) -> Result<()> {
        self.config.borrow_mut().send_buffer_size = size;
        Ok(())
    }

    /// Returns the size of the TCP send buffer for this socket.
    ///
    /// On most operating systems, this is the value of the SO_SNDBUF socket option.
    pub fn send_buffer_size(&self) -> Result<u32> {
        Ok(self.config.borrow().send_buffer_size)
    }

    /// Sets the size of the TCP receive buffer on this socket.
    ///
    /// On most operating systems, this sets the SO_RCVBUF socket option.
    pub fn set_recv_buffer_size(&self, size: u32) -> Result<()> {
        self.config.borrow_mut().recv_buffer_size = size;
        Ok(())
    }

    /// Returns the size of the TCP receive buffer for this socket.
    ///
    /// On most operating systems, this is the value of the SO_RCVBUF socket option.
    pub fn recv_buffer_size(&self) -> Result<u32> {
        Ok(self.config.borrow().recv_buffer_size)
    }

    /// Sets the linger duration of this socket by setting the SO_LINGER option.
    ///
    /// This option controls the action taken when a stream has unsent messages
    /// and the stream is closed. If SO_LINGER is set, the system shall block the process
    /// until it can transmit the data or until the time expires.
    ///
    /// If SO_LINGER is not specified, and the socket is closed, the system handles the call
    /// in a way that allows the process to continue as quickly as possible.
    pub fn set_linger(&self, dur: Option<Duration>) -> Result<()> {
        self.config.borrow_mut().linger = dur;
        Ok(())
    }

    /// Reads the linger duration for this socket by getting the SO_LINGER option.
    ///
    /// For more information about this option, see [set_linger].
    pub fn linger(&self) -> Result<Option<Duration>> {
        Ok(self.config.borrow().linger)
    }

    /// Gets the local address of this socket.
    ///
    /// Will fail on windows if called before bind.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.config.borrow().addr)
    }

    /// Returns the value of the SO_ERROR option.
    pub fn take_error(&self) -> Result<Option<Error>> {
        Ok(None)
    }

    /// Binds the socket to the given address.
    ///
    /// This calls the bind(2) operating-system function.
    /// Behavior is platform specific. Refer to the target platform’s documentation for more details.
    pub fn bind(&self, addr: SocketAddr) -> Result<()> {
        if self.expect_v4 != addr.ip().is_ipv4() {
            return Err(Error::new(ErrorKind::Other, "Expected other ip typ"));
        }

        self.config.borrow_mut().addr = addr;
        Ok(())
    }

    /// Establishes a TCP connection with a peer at the specified socket address.
    /// 
    /// The TcpSocket is consumed. Once the connection is established, 
    /// a connected TcpStream is returned. 
    /// If the connection fails, the encountered error is returned.
    /// 
    /// This calls the connect(2) operating-system function. 
    /// Behavior is platform specific. 
    /// Refer to the target platform’s documentation for more details.
    pub async fn connect(self, peer: SocketAddr) -> Result<TcpStream> {
        let this = IOContext::with_current(|ctx| {
            ctx.tcp_bind_stream(peer, Some(self.config.into_inner()))
        })?;

        loop {
            // Initiate connect by sending a message (better repeat)
            let interest = IOInterest::TcpConnect((this.inner.local_addr, this.inner.peer_addr));
            match interest.await {
                Ok(()) => {},
                Err(e) => {
                    return Err(e)
                }
            }

            let acked = IOContext::with_current(|ctx| { 
                if let Some(handle) = ctx.tcp_streams.get(&(this.inner.local_addr, this.inner.peer_addr)) {
                    Ok(handle.acked)
                } else {
                    Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpStream",
                    ))
                }
            })?;

            if acked {
                return Ok(this)
            }
        }
    }

    /// Converts the socket into a TcpListener.
    /// 
    /// backlog defines the maximum number of pending connections 
    /// are queued by the operating system at any given time. 
    /// Connection are removed from the queue with TcpListener::accept. 
    /// When the queue is full, the operating-system will start rejecting connections.
    /// 
    /// This calls the listen(2) operating-system function, marking the socket as a 
    /// passive socket. Behavior is platform specific. 
    /// Refer to the target platform’s documentation for more details.
    pub fn listen(self, backlog: u32) -> Result<TcpListener> {
        self.config.borrow_mut().listen_backlog = backlog;
        IOContext::with_current(|ctx| {
            ctx.tcp_bind_listener(self.local_addr()?, Some(self.config.into_inner()))
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Not implemented in simulation context")]
    #[allow(unused)]
    pub fn from_std_stream(std_stream: std::net::TcpStream) -> TcpSocket {
        unimplemented!()
    }
}
