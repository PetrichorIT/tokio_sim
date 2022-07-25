use super::{addr::*, Result};
use std::cell::RefCell;
use std::io::{Error, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr};
use std::task::{Context, Waker, Poll};
use std::future::Future;
use std::pin::Pin;

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

thread_local! {
    static IOCTX: RefCell<Option<IOContext>> = const { RefCell::new(None) }
}

/// The IO Context
#[derive(Debug)]
pub struct IOContext {
    interfaces: Vec<Interface>,

    udp_sockets: Vec<UdpSocketBuffer>,
    udp_read_interest: Vec<ReadInterest>,
}

impl IOContext {
    ///
    /// Creates a new instance.
    ///
    pub fn new(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            interfaces: vec![Interface::loopback(), Interface::en0(ether, v4)],

            udp_sockets: Vec::new(),
            udp_read_interest: Vec::new(),
        }
    }

    /// PUB
    pub fn yield_outgoing()-> Vec<UdpMessage> {
        IOContext::with_current(|ctx| ctx._yield_outgoing())
    }

    /// HUH
    pub(crate) fn _yield_outgoing(&mut self) -> Vec<UdpMessage> {
        let mut r = Vec::new();
        for socket in self.udp_sockets.iter_mut() {
            r.append(&mut socket.outgoing)
        }
        r
    }


    /// PUB
    pub fn process_incoming(message: UdpMessage) {
        IOContext::with_current(|ctx| ctx._process_incoming(message))
    }

    /// HUH
    pub(crate) fn _process_incoming(&mut self, message: UdpMessage) {
        let to = message.to;
        if let Some(handle) = self.udp_sockets.iter_mut().find(|h| *h.state.addr() == to) {
            handle.incoming.push(message);

            let mut i = 0;
            while i < self.udp_read_interest.len() {
                if self.udp_read_interest[i].addr == to {
                    let int = self.udp_read_interest.swap_remove(i);
                    int.waker.wake();
                } else {
                    i += 1;
                }
            }
        }


    }

    pub(crate) fn with_current<R>(f: impl FnOnce(&mut IOContext) -> R) -> R {
        IOCTX.with(|c| f(c.borrow_mut().as_mut().unwrap()))
    }
}

impl IOContext {
    pub(crate) fn udp_bind(&mut self, addr: SocketAddr) -> Result<UdpSocket> {
        // (1) check loopback;
        if addr.ip().is_loopback() {
            if let Some(lo0) = self.interfaces.iter().find(|i| i.flags.loopback) {
                // Find buffer
                if self.udp_sockets.iter().any(|s| *s.state.addr() == addr) {
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
                    outgoing: Vec::with_capacity(8)
                };

                self.udp_sockets.push(buf);

                return Ok(UdpSocket { addr });
            }
        }

        Err(Error::new(ErrorKind::Other, "TODO"))
    }

    
}

pub(crate) struct IOInterest {
    addr: SocketAddr
}

impl Future for IOInterest {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.udp_sockets.iter().find(|s| *s.state.addr() == self.addr) {
                if handle.incoming.is_empty() {
                    ctx.udp_read_interest.push(ReadInterest {
                        addr: self.addr,
                        waker: cx.waker().clone(),
                    });
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }

            } else {
                panic!("HUH")
            }
        })
    }
}

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
            Error::new(
                ErrorKind::InvalidInput,
                "could not resolve to any address",
            )
        }))
    }

    fn bind_addr(addr: SocketAddr) -> Result<Self> {
        IOContext::with_current(|ctx| ctx.udp_bind(addr))
    }

    /// Send
    pub async fn send_to(&self, buf: &[u8], target: impl ToSocketAddrs) -> Result<usize> {
        let addr = to_socket_addrs(target).await;
        let first = addr.unwrap().next().unwrap();

        IOContext::with_current(|ctx| {
            if let Some(handle) = ctx.udp_sockets.iter_mut().find(|s| *s.state.addr() == self.addr) {
                handle.outgoing.push(UdpMessage {
                    from: self.addr,
                    to: first,
                    buffer: Vec::from(buf)
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
                if let Some(handle) = ctx.udp_sockets.iter_mut().find(|s| *s.state.addr() == self.addr) {
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

                    return Ok((wrt, msg.from))
                },
                None => {}
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct UdpSocketBuffer {
    state: UdpSocketState,
    incoming: Vec<UdpMessage>,
    outgoing: Vec<UdpMessage>,
}

#[derive(Debug)]
pub(crate) enum UdpSocketState {
    Bound(SocketAddr),
    Connected(SocketAddr, SocketAddr),
}

impl UdpSocketState {
    pub(crate) fn addr(&self) -> &SocketAddr {
        match self {
            Self::Bound(ref addr) => addr,
            Self::Connected(ref addr, _) => addr,
        }
    }
}

/// WELL its a UDP message soo
#[derive(Debug)]
#[allow(missing_docs)]
pub struct UdpMessage {
    pub buffer: Vec<u8>,
    pub from: SocketAddr,
    pub to: SocketAddr,
}

#[derive(Debug)]
pub(crate) struct ReadInterest {
    addr: SocketAddr,
    waker: Waker,
}

pub use interface::*;
mod interface {
    use std::net::{Ipv4Addr, Ipv6Addr};

    /// A network interface.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct Interface {
        /// The name of the interface
        pub name: String,
        /// The flags.
        pub flags: InterfaceFlags,
        /// The associated addrs.
        pub addrs: Vec<InterfaceAddr>,
        /// The status.s
        pub status: InterfaceStatus,
    }

    impl Interface {
        /// Creates a loopback interface
        pub fn loopback() -> Self {
            Self {
                name: "lo0".to_string(),
                flags: InterfaceFlags::loopback(),
                addrs: Vec::from(InterfaceAddr::loopback()),
                status: InterfaceStatus::Active,
            }
        }

        /// Creates a loopback interface
        pub fn en0(ether: [u8; 6], v4: Ipv4Addr) -> Self {
            Self {
                name: "en0".to_string(),
                flags: InterfaceFlags::en0(),
                addrs: Vec::from(InterfaceAddr::en0(ether, v4)),
                status: InterfaceStatus::Active,
            }
        }
    }

    /// The flags of an interface.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[allow(missing_docs)]
    pub struct InterfaceFlags {
        pub up: bool,
        pub loopback: bool,
        pub running: bool,
        pub muticast: bool,
        pub p2p: bool,
        pub broadcast: bool,
        pub smart: bool,
        pub simplex: bool,
        pub promisc: bool,
    }

    impl InterfaceFlags {
        /// The flags for the loopback interface
        pub const fn loopback() -> Self {
            Self {
                up: true,
                loopback: true,
                running: true,
                muticast: true,
                p2p: false,
                broadcast: false,
                smart: false,
                simplex: false,
                promisc: false,
            }
        }

        /// The flags for a simple interface
        pub const fn en0() -> Self {
            Self {
                up: true,
                loopback: false,
                running: true,
                muticast: true,
                p2p: false,
                broadcast: true,
                smart: true,
                simplex: true,
                promisc: false,
            }
        }
    }

    /// The status of a network interface
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum InterfaceStatus {
        /// The interface is active and can be used.
        Active,
        /// The interface is only pre-configures not really there.
        #[default]
        Inactive,
    }

    /// A interface addr.
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum InterfaceAddr {
        /// A hardware ethernet address.
        Ether {
            /// The MAC addr.
            addr: [u8; 6],
        },
        /// An Ipv4 declaration
        Inet {
            /// The net addr,
            addr: Ipv4Addr,
            /// The mask to create the subnet.
            netmask: Ipv4Addr,
        },
        /// The Ipv6 declaration
        Inet6 {
            /// The net addr.
            addr: Ipv6Addr,
            /// The mask to create the subnet
            prefixlen: usize,
            /// Scoping.
            scope_id: Option<usize>,
        },
    }

    impl InterfaceAddr {
        /// Returns the addrs for a loopback interface.
        pub const fn loopback() -> [Self; 3] {
            [
                InterfaceAddr::Inet {
                    addr: Ipv4Addr::LOCALHOST,
                    netmask: Ipv4Addr::new(255, 0, 0, 0),
                },
                InterfaceAddr::Inet6 {
                    addr: Ipv6Addr::LOCALHOST,
                    prefixlen: 128,
                    scope_id: None,
                },
                InterfaceAddr::Inet6 {
                    addr: Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1),
                    prefixlen: 64,
                    scope_id: Some(0x1),
                },
            ]
        }

        /// Returns the addrs for a loopback interface.
        pub const fn en0(ether: [u8; 6], v4: Ipv4Addr) -> [Self; 2] {
            [
                InterfaceAddr::Ether { addr: ether },
                InterfaceAddr::Inet {
                    addr: v4,
                    netmask: Ipv4Addr::new(255, 255, 255, 0),
                },
            ]
        }
    }
}
