use super::super::TcpStreamInner;
use super::TcpStream;

use crate::sim::net::{IOContext, IOInterest, IOInterestGuard, Result};
use crate::io::{Ready, ReadBuf, AsyncRead, AsyncWrite, Interest};

use std::io::{Error, ErrorKind, IoSliceMut, IoSlice};
use std::sync::Arc;
use std::task::*;
use std::pin::Pin;
use std::net::SocketAddr;

/// Owned read half of a [TcpStream], created by [into_split](super::TcpStream::into_split).
///
/// Reading from an [OwnedReadHalf] is usually done using the convenience methods 
/// found on the [AsyncReadExt](crate::io::AsyncReadExt) trait.
#[derive(Debug)]
pub struct OwnedReadHalf {
    pub(super) inner: Arc<TcpStreamInner>,
}

/// Owned read half of a [TcpStream], created by [into_split](super::TcpStream::into_split).
///
/// Reading from an [OwnedReadHalf] is usually done using the convenience methods 
/// found on the [AsyncReadExt](crate::io::AsyncReadExt) trait.
#[derive(Debug)]
pub struct OwnedWriteHalf {
    pub(super) inner: Arc<TcpStreamInner>,
}

/// Error indicating that two halves were
/// not from the same socket, and thus could not be reunited.
#[derive(Debug)]
pub struct ReuniteError(pub OwnedReadHalf, pub OwnedWriteHalf);

impl OwnedReadHalf {
    /// Attempts to put the two halves of a [TcpStream] back together
    /// and recover the original socket.
    /// Succeeds only if the two halves originated from the same call to [into_split](TcpStream::into_split).
    pub fn reunite(self, other: OwnedWriteHalf) -> std::result::Result<TcpStream, ReuniteError> {
        if Arc::ptr_eq(&self.inner, &other.inner) {
            Ok(TcpStream { inner: self.inner })
        } else {
            Err(ReuniteError(self, other))
        }
    }

    /// Receives data on the socket from the remote address to which it is connected, 
    /// without removing that data from the queue.
    /// On success, returns the number of bytes peeked.
    /// 
    /// Successive calls return the same data. 
    /// This is accomplished by passing MSG_PEEK as a flag to the underlying recv system call.
    pub async fn peek(&self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let interest = IOInterest::TcpRead((self.inner.local_addr, self.inner.peer_addr));
            interest.await?;

            let n = IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.tcp_streams.get_mut(&(self.inner.local_addr, self.inner.peer_addr)) {
                    Ok(handle.incoming.peek(buf))
                } else {
                    Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpStream",
                    ))
                }
            })?;

            if n != 0 { return Ok(n) }
        }
    }

    /// Returns the local address that this stream is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.local_addr)
    }

    /// Returns the remote address that this stream is connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.peer_addr)
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::TcpStream")]
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
    /// It can be used to concurrently read / write to the same socket on a single task without splitting the socket.
    /// 
    /// This function is equivalent to TcpStream::ready.
    pub async fn ready(&self, interest: Interest) -> Result<Ready> {
        let (io_interest, ready) = interest.tcp_io_interest(self.inner.local_addr, self.inner.peer_addr);
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

    /// Tries to read data from the stream into the provided buffer, 
    /// returning how many bytes were read.
    /// 
    /// Receives any pending data from the socket but does not wait for new data to arrive. 
    /// On success, returns the number of bytes read. 
    /// Because try_read() is non-blocking, the buffer does not have to be stored by the async task 
    /// and can exist entirely on the stack.
    pub fn try_read(&self, buf: &mut [u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.inner.local_addr, self.inner.peer_addr)) {
                let n = handle.incoming.read(buf);
                if n > 0 {
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
    #[deprecated(note = "Cannot create simulated socket from std::net::TcpStream")]
    #[allow(unused)]
    pub fn try_read_buf<B>(&self, buf: &mut B) -> Result<usize> {
        unimplemented!()
    }

    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::TcpStream")]
    #[allow(unused)]
    pub fn try_read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> Result<usize> {
        unimplemented!()
    }
}

impl OwnedWriteHalf {

    /// Attempts to put the two halves of a [TcpStream] back together
    /// and recover the original socket.
    /// Succeeds only if the two halves originated from the same call to [into_split](TcpStream::into_split).
    pub fn reunite(self, other: OwnedReadHalf) -> std::result::Result<TcpStream, ReuniteError> {
        if Arc::ptr_eq(&self.inner, &other.inner) {
            Ok(TcpStream { inner: self.inner })
        } else {
            Err(ReuniteError(other, self))
        }
    }

    /// Destroys the write half, but don’t close the write half of the stream until the read half is dropped. 
    /// If the read half has already been dropped, this closes the stream.
    pub fn forget(self) {
        drop(self);
    }
    

    /// Returns the local address that this stream is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.local_addr)
    }

    /// Returns the remote address that this stream is connected to.
    pub fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.inner.peer_addr)
    }

    /// Waits for any of the requested ready states.
    /// 
    /// This function is usually paired with try_read() or try_write(). 
    /// It can be used to concurrently read / write to the same socket on a single task without splitting the socket.
    /// 
    /// This function is equivalent to TcpStream::ready.
    pub async fn ready(&self, interest: Interest) -> Result<Ready> {
        let (io_interest, ready) = interest.tcp_io_interest(self.inner.local_addr, self.inner.peer_addr);
        io_interest.await?;
        Ok(ready)
    }

    /// Waits for the socket to become writable.
    /// 
    /// This function is equivalent to `ready(Interest::WRITABLE)` and is usually paired with `try_write()`.
    pub async fn writable(&self) -> Result<()> {
        self.ready(Interest::WRITABLE).await?;
        Ok(())
    }

    /// Tries to write a buffer to the stream, returning how many bytes were written.
    /// 
    /// The function will attempt to write the entire contents of buf, but only part of the buffer may be written.
    /// 
    /// This function is usually paired with writable().
    pub fn try_write(&self, buf: &[u8]) -> Result<usize> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.inner.local_addr, self.inner.peer_addr)) {
                if let Err(rem) = handle.outgoing.write(buf) {
                    Ok(buf.len() - rem.len())
                } else {
                    Ok(buf.len())
                }
            } else {
                Err(Error::new(ErrorKind::Other, "Simulation context has lost TcpStream"))
            }
        })
    }

    
    /// DEPRECATED
    #[deprecated(note = "Cannot create simulated socket from std::net::TcpStream")]
    #[allow(unused)]
    pub fn try_write_vectored(&self, bufs: &[IoSlice<'_>]) -> Result<usize> {
        unimplemented!()
    }
}

impl AsyncRead for OwnedReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>
    ) -> Poll<Result<()>> {
        // Whenever polled -- try to fill the buffer first
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.inner.local_addr, self.inner.peer_addr)) {
                let old = buf.remaining();
                let remaining = handle.incoming.read_buf(buf);
                if old != remaining {
                    Poll::Ready(Ok(()))
                } else {
                    let interest = IOInterest::TcpRead((self.inner.local_addr, self.inner.peer_addr));
                    handle.interests.push(IOInterestGuard {
                        interest: interest.clone(),
                        waker: cx.waker().clone(),
                    });
                    Poll::Pending
                }
            } else {
                Poll::Ready(Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped TcpStream",
                )))
            }
        })
    }
}

impl AsyncWrite for OwnedWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<Result<usize>> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.tcp_streams.get_mut(&(self.inner.local_addr, self.inner.peer_addr)) {
                if let Err(rem) = handle.outgoing.write(buf) {
                    if buf.len() == rem.len() {
                        // must be exceeded buffer size
                        ctx.tick_wakeups.push(cx.waker().clone());
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(buf.len() - rem.len()))
                    }
                } else {
                    Poll::Ready(Ok(buf.len()))
                }
            } else {
                Poll::Ready(Err(Error::new(ErrorKind::Other, "Simulation context has lost TcpStream"))) 
            }
        })
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

