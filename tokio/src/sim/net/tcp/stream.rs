use super::super::{addr::*, Result, IOContext, IOIntent, IOInterest, IOInterestGuard, Interest, TcpMessage};
use crate::io::{Error, ErrorKind, Ready, ReadBuf, AsyncRead, AsyncWrite};

use std::net::SocketAddr;
use std::task::*;
use std::time::Duration;
use std::io::{IoSlice, IoSliceMut};
use std::pin::Pin;

/// A TCP Stream.
#[derive(Debug)]
pub struct TcpStream {
    pub(crate) local_addr: SocketAddr,
    pub(crate) peer_addr: SocketAddr,
}

impl TcpStream {
    /// Opens a TCP connection to a remote host.
    /// 
    /// addr is an address of the remote host. 
    /// Anything which implements the ToSocketAddrs trait can be supplied as the address. 
    /// If addr yields multiple addresses, connect will be attempted with each of the addresses 
    /// until a connection is successful. If none of the addresses result in a successful connection, 
    /// the error returned from the last connection attempt (the last address) is returned.
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> Result<TcpStream> {
        let addrs = to_socket_addrs(addr).await?;
        let mut last_err = None;

        for peer in addrs {
            let this = IOContext::with_current(|ctx| {
                ctx.tcp_bind_stream(peer)
            })?;
    
            loop {
                // Initiate connect by sending a message (better repeat)
                let interest = IOInterest::TcpConnect((this.local_addr, this.peer_addr));
                match interest.await {
                    Ok(()) => {},
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
    
                let acked = IOContext::with_current(|ctx| { 
                    if let Some(handle) = ctx.tcp_streams.get(&(this.local_addr, this.peer_addr)) {
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

        Err(last_err.unwrap_or(Error::new(ErrorKind::Other, "No address worked")))
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn from_std(stream: TcpStream) -> Result<TcpStream> { unimplemented!() }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn into_std(self) -> Result<TcpStream> { unimplemented!() }

    /// Returns the local address that this stream is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.local_addr)
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn take_error(&self) -> Result<Option<Error>> {
        unimplemented!()
    }

    /// Returns the remote address that this stream is connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.peer_addr)
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn poll_peek(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<Result<usize>> {
        unimplemented!()
    }

    /// Waits for any of the requested ready states.
    /// 
    /// This function is usually paired with try_read() or try_write(). 
    /// It can be used to concurrently read / write to the same socket on a single task 
    /// without splitting the socket.
    pub async fn ready(&self, interest: Interest) -> Result<Ready> {
        let (io_interest, ready) = interest.tcp_io_interest(self.local_addr, self.peer_addr);
        io_interest.await?;
        Ok(ready)
    }

    /// Waits for the socket to become readable.
    /// 
    /// This function is equivalent to ready(Interest::READABLE) and is usually paired with try_read().
    pub async fn readable(&self) -> Result<()> {
        self.ready(Interest::READABLE).await?;
        Ok(())
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<Result<()>> {
        unimplemented!()
    }

    /// Tries to read data from the stream into the provided buffer, 
    /// returning how many bytes were read.
    /// 
    /// Receives any pending data from the socket but does not wait for new data to arrive. 
    /// On success, returns the number of bytes read. 
    /// Because try_read() is non-blocking, the buffer does not have to be stored by the async task 
    /// and can exist entirely on the stack.
    pub fn try_read(&self, buf: &mut [u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                if let Some(msg) = handle.incoming.pop_front() {
                    let n = msg.content.len().min(buf.len());
                    for i in 0..n {
                        buf[i] = msg.content[i];
                    }
                    Ok(n)
                } else {
                    Err(Error::new(ErrorKind::WouldBlock, "No message could be received non-blocking"))
                }
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn try_read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        unimplemented!()
    }

    /// Waits for the socket to become writable.
    /// 
    /// This function is equivalent to `ready(Interest::WRITABLE)` and 
    /// is usually paired with `try_write()`.
    pub async fn writable(&self) -> Result<()> {
        self.ready(Interest::WRITABLE).await?;
        Ok(())
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<Result<()>> {
        unimplemented!()
    }

    /// Try to write a buffer to the stream, returning how many bytes were written.
    /// 
    /// The function will attempt to write the entire contents of `buf`, 
    /// but only part of the buffer may be written.
    pub fn try_write(&self, buf: &[u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            let content = Vec::from(buf);
            let msg = TcpMessage {
                src_addr: self.local_addr,
                dest_addr: self.peer_addr,
                content,
                ttl: 64,
            };
            
            ctx.intents.push(IOIntent::TcpSendPacket(msg));
            Ok(buf.len())
        })
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn try_write_vectored(&self, bufs: &[IoSlice<'_>]) -> Result<usize> {
        unimplemented!()
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::UdpSocket")]
    #[allow(unused)]
    pub fn try_io<R>(
        &self,
        interest: Interest,
        f: impl FnOnce() -> Result<R>
    ) -> Result<R> {
        unimplemented!()
    }

    /// Receives data on the socket from the remote address to which it is connected, 
    /// without removing that data from the queue.
    /// On success, returns the number of bytes peeked.
    /// 
    /// Successive calls return the same data. 
    /// This is accomplished by passing MSG_PEEK as a flag to the underlying recv system call.
    pub async fn peek(&self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let interest = IOInterest::TcpRead((self.local_addr, self.peer_addr));
            interest.await?;

            let n = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.tcp_streams.get(&(self.local_addr, self.peer_addr)) {
                    if let Some(msg) = handle.incoming.front() {
                        let n = msg.content.len().min(buf.len());
                        for i in 0..n {
                            buf[i] = msg.content[i]
                        }
                        Ok(n)
                    } else {
                        Err(Error::new(ErrorKind::WouldBlock, "WouldBlock"))
                    }
                } else {
                    Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpStream",
                    ))
                }
            });

            match n {
                Ok(n) => return Ok(n),
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Gets the value of the TCP_NODELAY option on this socket.
    /// 
    /// For more information about this option, see [set_nodelay].
    pub fn nodelay(&self) -> Result<bool> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get(&(self.local_addr, self.peer_addr)) {
                Ok(handle.config.nodelay)
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// Sets the value of the TCP_NODELAY option on this socket.
    /// 
    /// If set, this option disables the Nagle algorithm. 
    /// This means that segments are always sent as soon as possible, 
    /// even if there is only a small amount of data. When not set, 
    /// data is buffered until there is a sufficient amount to send out, 
    /// thereby avoiding the frequent sending of small packets.
    pub fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                handle.config.nodelay = nodelay;
                Ok(())
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// Reads the linger duration for this socket by getting the SO_LINGER option.
    /// 
    /// For more information about this option, see [set_linger].
    pub fn linger(&self) -> Result<Option<Duration>> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get(&(self.local_addr, self.peer_addr)) {
                Ok(handle.config.linger)
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// Sets the linger duration of this socket by setting the SO_LINGER option.
    /// 
    /// This option controls the action taken when a stream has unsent messages and 
    /// the stream is closed. If SO_LINGER is set, the system shall block the process 
    /// until it can transmit the data or until the time expires.
    pub fn set_linger(&self, dur: Option<Duration>) -> Result<()> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                handle.config.linger = dur;
                Ok(())
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// Gets the value of the IP_TTL option for this socket.
    /// 
    /// For more information about this option, see [set_ttl].
    pub fn ttl(&self) -> Result<u32> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get(&(self.local_addr, self.peer_addr)) {
                Ok(handle.config.ttl)
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }

    /// Sets the value for the IP_TTL option on this socket.
    /// 
    /// This value sets the time-to-live field that is used in every packet sent from this socket.
    pub fn set_ttl(&self, ttl: u32) -> Result<()> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                handle.config.ttl = ttl;
                Ok(())
            } else {
                Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                ))
            }
        })
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<Result<()>> {
        // Await
        let interest = IOInterest::TcpRead((self.local_addr, self.peer_addr));
        let result = IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                if handle.incoming.is_empty() {
                    handle.interests.push(IOInterestGuard {
                        interest: interest.clone(),
                        waker: cx.waker().clone(),
                    });

                    Poll::Pending
                } else {
                    println!("{:?}", handle.incoming);

                    Poll::Ready(Ok(()))
                }
            } else {
                Poll::Ready(Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                )))
            }
        });

        match result {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Ready(Ok(())) => {
                let msg = IOContext::with_current(|ctx| {
                    if let Some(handle) = ctx.tcp_streams.get_mut(&(self.local_addr, self.peer_addr)) {
                        println!("{:?}", handle.incoming);

                        if let Some(msg) = handle.incoming.pop_front() {
                            Ok(msg)
                        } else {
                            Err(Error::new(ErrorKind::WouldBlock, "WouldBlock"))
                        }
                    } else {
                        Err(Error::new(
                            ErrorKind::Other,
                            "Simulation context has dropped TcpStream",
                        ))
                    }
                });

                match msg {
                    Ok(msg) => {
                        let pre_filled = buf.filled().len();
                        let buffer = buf.initialize_unfilled();

                        let n = msg.content.len().min(buffer.len());
                        for i in 0..n {
                            buffer[i] = msg.content[i];
                        }

                        buf.set_filled(pre_filled + n);

                        Poll::Ready(Ok(()))
                    },
                    Err(e) if e.kind() == ErrorKind::WouldBlock => Poll::Pending,
                    Err(e) => return Poll::Ready(Err(e))
                }
            }
        }
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<Result<usize>> {
        Poll::Ready(self.try_write(buf))
    }
    fn poll_flush(
        self: Pin<&mut Self>,
        _: &mut Context<'_>
    ) -> Poll<Result<()>> {
        // NOP
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: Pin<&mut Self>,
        _: &mut Context<'_>
    ) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}