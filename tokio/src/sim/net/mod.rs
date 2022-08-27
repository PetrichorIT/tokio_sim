use std::collections::HashMap;
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

pub(crate) use std::io::Result;

mod interface;
pub use interface::*;

mod addr;
pub use addr::*;

mod lookup_host;
pub use lookup_host::*;

mod udp;
pub use udp::*;

mod tcp;
pub use tcp::*;

mod interest;
pub use interest::*;

/// A action that must be managed by the simulation core since it supercedes
/// the limits of the current network node.
#[derive(Debug)]
pub enum IOIntent {
    /// The intent to forward a udp packet onto the network layer.
    UdpSendPacket(UdpMessage),

    /// The intent to perform a tcp handshake.
    TcpConnect(TcpConnectMessage),
    /// A timeout of a connection setup
    TcpConnectTimeout(TcpConnectMessage, Duration),

    /// The intent to forward a tcp packet onto the network layer.
    TcpSendPacket(TcpMessage),
    /// The intent to shut down a tcp connection
    TcpShutdown(),

    /// The intent to look up a non trivial dns.
    DnsLookup(),
}

// # IO Interest

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IOInterest {
    UdpRead(SocketAddr),
    UdpWrite(SocketAddr),

    TcpAccept(SocketAddr),
    TcpConnect((SocketAddr, SocketAddr)),
    TcpRead((SocketAddr, SocketAddr)),
    TcpWrite((SocketAddr, SocketAddr)),
}

#[derive(Debug, Clone)]
pub(super) struct IOInterestGuard {
    waker: Waker,
    interest: IOInterest,
}

impl Future for IOInterest {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // let this = Pin::into_inner(self.into_inner();

        match *self {
            // == UDP ==
            IOInterest::UdpRead(ref sock) => IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.udp_sockets.get_mut(sock) {
                    if handle.incoming.is_empty() {
                        handle.interests.push(IOInterestGuard {
                            interest: self.clone(),
                            waker: cx.waker().clone(),
                        });

                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(()))
                    }
                } else {
                    Poll::Ready(Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped UdpSocket",
                    )))
                }
            }),
            IOInterest::UdpWrite(_) => Poll::Ready(Ok(())),

            // == TCP ==
            IOInterest::TcpAccept(ref sock) => IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.tcp_listeners.get_mut(sock) {
                    if handle.incoming.is_empty() {
                        handle.interests.push(IOInterestGuard {
                            interest: self.clone(),
                            waker: cx.waker().clone(),
                        });

                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(()))
                    }
                } else {
                    Poll::Ready(Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpListener",
                    )))
                }
            }),

            IOInterest::TcpConnect(ref addr_peer) => IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.tcp_streams.get_mut(addr_peer) {
                    if handle.acked {
                        Poll::Ready(Ok(()))
                    } else {
                        if handle.connection_failed {
                            handle.connection_failed = false;

                            Poll::Ready(Err(Error::new(
                                ErrorKind::NotConnected,
                                "Connection timed out",
                            )))
                        } else {
                            let (addr, peer) = *addr_peer;

                            let msg = TcpConnectMessage::ClientInitiate {
                                client: addr,
                                server: peer,
                            };

                            ctx.intents.push(IOIntent::TcpConnect(msg));
                            ctx.intents.push(IOIntent::TcpConnectTimeout(
                                msg,
                                handle.config.connect_timeout,
                            ));

                            handle.interests.push(IOInterestGuard {
                                interest: self.clone(),
                                waker: cx.waker().clone(),
                            });

                            Poll::Pending
                        }
                    }
                } else {
                    Poll::Ready(Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpStream",
                    )))
                }
            }),

            // Stream operations
            IOInterest::TcpRead(ref addr_peer) => IOContext::with_current(|ctx| {
                if let Some(handle) = ctx.tcp_streams.get_mut(addr_peer) {
                    if handle.incoming.is_empty() {
                        handle.interests.push(IOInterestGuard {
                            interest: self.clone(),
                            waker: cx.waker().clone(),
                        });

                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(()))
                    }
                } else {
                    Poll::Ready(Err(Error::new(
                        ErrorKind::Other,
                        "Simulation context has dropped TcpStream",
                    )))
                }
            }),
            IOInterest::TcpWrite(_) => Poll::Ready(Ok(())),
        }
    }
}

// # IO Context

/// A context managing a simulated network node.
#[derive(Debug)]
pub struct IOContext {
    /// Node information
    pub(self) interfaces: Vec<Interface>,

    /// Outgoing
    pub(self) intents: Vec<IOIntent>,

    /// Registry
    pub(super) udp_sockets: HashMap<SocketAddr, UdpSocketHandle>,

    pub(self) tcp_listeners: HashMap<SocketAddr, TcpListenerHandle>,
    pub(self) tcp_streams: HashMap<(SocketAddr, SocketAddr), TcpStreamHandle>,
    pub(self) tcp_next_port: u16,
}

impl IOContext {
    /// Creates a new IO Context.
    pub fn new(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            interfaces: vec![Interface::loopback(), Interface::en0(ether, v4)],

            intents: Vec::new(),

            udp_sockets: HashMap::new(),
            tcp_listeners: HashMap::new(),
            tcp_streams: HashMap::new(),
            tcp_next_port: 1014,
        }
    }

    /// Sets the IO Context
    pub fn set(self) {
        use super::ctx::IOCTX;
        IOCTX.with(|c| c.borrow_mut().io = Some(self))
    }

    pub(crate) fn try_with_current<R>(f: impl FnOnce(&mut IOContext) -> R) -> Option<R> {
        use super::ctx::IOCTX;
        IOCTX.with(|c| {
            if let Some(io) = c.borrow_mut().io.as_mut() {
                Some(f(io))
            } else {
                None
            }
        })
    }

    pub(crate) fn with_current<R>(f: impl FnOnce(&mut IOContext) -> R) -> R {
        use super::ctx::IOCTX;
        IOCTX.with(|c| f(c.borrow_mut().io.as_mut().unwrap()))
    }

    pub(crate) fn yield_intents(&mut self) -> Vec<IOIntent> {
        let mut swap = Vec::new();
        std::mem::swap(&mut swap, &mut self.intents);
        swap
    }

    ///
    /// Processes a UDP packet.
    ///
    pub fn process_udp(&mut self, msg: UdpMessage) {
        let sock = msg.dest_addr;

        if let Some(handle) = self.udp_sockets.get_mut(&sock) {
            handle.incoming.push(msg);
            handle.interests.drain(..).for_each(|w| w.waker.wake())
        } else {
            println!("Dropping UDP Message :  {:?}", msg);
            for info in self.udp_sockets() {
                println!("- {:?}", info)
            }
        }
    }

    ///
    /// Processes a TCP Connection Message.
    ///
    pub fn process_tcp_connect(&mut self, msg: TcpConnectMessage) {
        match msg {
            // Server side code
            TcpConnectMessage::ClientInitiate { client, server } => {
                // look for listener
                if let Some(handle) = self.tcp_listeners.get_mut(&server) {
                    handle.incoming.push(TcpListenerPendingConnection {
                        local_addr: server,
                        peer_addr: client,
                    });

                    // Wake up
                    let mut i = 0;
                    while i < handle.interests.len() {
                        if matches!(handle.interests[i].interest, IOInterest::TcpAccept(_)) {
                            let w = handle.interests.swap_remove(i);
                            w.waker.wake();
                        } else {
                            i += 1;
                        }
                    }

                    // Ack to client
                    self.intents
                        .push(IOIntent::TcpConnect(TcpConnectMessage::ServerAcknowledge {
                            client,
                            server,
                        }))
                }
            }
            // Client side code
            TcpConnectMessage::ServerAcknowledge { client, server } => {
                if let Some(handle) = self.tcp_streams.get_mut(&(client, server)) {
                    handle.acked = true;

                    let mut i = 0;
                    while i < handle.interests.len() {
                        if matches!(handle.interests[i].interest, IOInterest::TcpConnect(_)) {
                            let w = handle.interests.swap_remove(i);
                            w.waker.wake();
                        } else {
                            i += 1;
                        }
                    }
                }
            }
        }
    }

    ///
    /// Processa a tcp packet
    ///
    pub fn process_tcp_packet(&mut self, msg: TcpMessage) {
        if let Some(handle) = self.tcp_streams.get_mut(&(msg.dest_addr, msg.src_addr)) {
            handle.incoming.push(msg);

            let mut i = 0;
            while i < handle.interests.len() {
                if matches!(handle.interests[i].interest, IOInterest::TcpRead(_)) {
                    let w = handle.interests.swap_remove(i);
                    w.waker.wake();
                } else {
                    i += 1;
                }
            }
        }
    }

    ///
    /// Processes a timeout
    ///
    pub fn process_tcp_connect_timeout(&mut self, msg: TcpConnectMessage) {
        match msg {
            TcpConnectMessage::ClientInitiate { client, server } => {
                if let Some(handle) = self.tcp_streams.get_mut(&(client, server)) {
                    // If no connection was established
                    if !handle.acked {
                        handle.connection_failed = true;

                        let mut i = 0;
                        while i < handle.interests.len() {
                            if matches!(handle.interests[i].interest, IOInterest::TcpConnect(_)) {
                                let w = handle.interests.swap_remove(i);
                                w.waker.wake();
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

// === UDP ===

impl IOContext {
    /// Grants the information over the udp sockets.
    pub fn udp_sockets(&self) -> Vec<UdpSocketInfo> {
        self.udp_sockets.iter().map(|(_, v)| v.info()).collect()
    }

    pub(self) fn udp_bind(&mut self, addr: SocketAddr) -> Result<UdpSocket> {
        // (1) check loopback;
        if addr.ip().is_loopback() {
            if let Some(lo0) = self.interfaces.iter().find(|i| i.flags.loopback) {
                // Find buffer
                if self.udp_sockets.get(&addr).is_some() {
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
                let buf = UdpSocketHandle {
                    local_addr: addr,
                    state: UdpSocketState::Bound,
                    incoming: Vec::new(),

                    ttl: 64,
                    broadcast: false,
                    multicast_loop_v4: false,
                    multicast_loop_v6: false,
                    multicast_ttl_v4: 64,

                    interests: Vec::new(),
                };

                self.udp_sockets.insert(addr, buf);

                return Ok(UdpSocket { addr });
            }
        }

        Err(Error::new(ErrorKind::Other, "TODO"))
    }

    pub(self) fn udp_send(
        &mut self,
        src_addr: SocketAddr,
        dest_addr: SocketAddr,
        content: Vec<u8>,
    ) -> Result<()> {
        // (1) Check a socket exits
        let handle = match self.udp_sockets.get(&src_addr) {
            Some(v) => v,
            None => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped UdpSocket",
                ))
            }
        };

        // (2) Build Message
        let msg = UdpMessage {
            content,
            src_addr,
            dest_addr,
            ttl: handle.ttl,
        };

        // (3) Send
        self.intents.push(IOIntent::UdpSendPacket(msg));

        Ok(())
    }

    pub(self) fn udp_connect(&mut self, socket: SocketAddr, peer: SocketAddr) -> Result<()> {
        let handle = match self.udp_sockets.get_mut(&socket) {
            Some(v) => v,
            None => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped UdpSocket",
                ))
            }
        };

        handle.state = UdpSocketState::Connected(peer);
        Ok(())
    }

    pub(self) fn udp_peer(&self, socket: SocketAddr) -> Option<SocketAddr> {
        self.udp_sockets
            .get(&socket)
            .expect("Lost socket")
            .state
            .peer()
    }

    pub(self) fn udp_drop(&mut self, socket: SocketAddr) {
        println!("Dropping socket");
        self.udp_sockets.remove(&socket);
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub(super) struct UdpSocketHandle {
    pub(super) local_addr: SocketAddr,
    pub(super) state: UdpSocketState,
    pub(super) incoming: Vec<UdpMessage>,

    pub(super) ttl: u32,
    pub(super) broadcast: bool,
    pub(super) multicast_loop_v4: bool,
    pub(super) multicast_loop_v6: bool,
    pub(super) multicast_ttl_v4: u32,

    pub(super) interests: Vec<IOInterestGuard>,
}

impl UdpSocketHandle {
    pub(self) fn info(&self) -> UdpSocketInfo {
        UdpSocketInfo {
            addr: self.local_addr,
            peer: self.state.peer(),
            in_queue_size: self.incoming.len(),
            interest_queue_size: self.interests.len(),
        }
    }
}

/// A public info over UDP sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpSocketInfo {
    /// The address the socket is bound to
    pub addr: SocketAddr,
    /// The peer socket if one was defined.
    pub peer: Option<SocketAddr>,
    /// The number of waiting packets
    pub in_queue_size: usize,
    /// The number of waiting call.
    pub interest_queue_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub(super) enum UdpSocketState {
    #[default]
    Bound,
    Connected(SocketAddr),
}

impl UdpSocketState {
    pub(self) fn peer(&self) -> Option<SocketAddr> {
        match self {
            Self::Bound => None,
            Self::Connected(addr) => Some(*addr),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// A UDP Message in the network.
pub struct UdpMessage {
    /// The content byte-encoded.
    pub content: Vec<u8>,
    /// The senders bound address.
    pub src_addr: SocketAddr,
    /// The receivers address.
    pub dest_addr: SocketAddr,
    /// Time-To-Live
    pub ttl: u32,
}

// == TCP ==

impl IOContext {
    pub(self) fn tcp_bind_listener(&mut self, addr: SocketAddr) -> Result<TcpListener> {
        let addr = self.tcp_bind_addr(addr)?;

        let buf = TcpListenerHandle {
            local_addr: addr,
            incoming: Vec::new(),
            interests: Vec::new(),

            config: TcpSocketConfig::listener(addr),
        };

        self.tcp_listeners.insert(addr, buf);

        return Ok(TcpListener { addr });
    }

    pub(self) fn tcp_drop_listener(&mut self, addr: SocketAddr) {
        self.tcp_listeners.remove(&addr);
    }

    pub(self) fn tcp_accept(&mut self, addr: SocketAddr) -> Result<TcpStream> {
        if let Some(handle) = self.tcp_listeners.get_mut(&addr) {
            let con = match handle.incoming.pop() {
                Some(con) => con,
                None => return Err(Error::new(ErrorKind::WouldBlock, "WouldBlock")),
            };

            assert_eq!(con.local_addr, addr);

            let buf = TcpStreamHandle {
                local_addr: con.local_addr,
                peer_addr: con.peer_addr,

                acked: true,
                connection_failed: false,

                incoming: Vec::new(),
                config: handle.config.accept(con),
                interests: Vec::new(),
            };
            self.tcp_streams
                .insert((con.local_addr, con.peer_addr), buf);
            Ok(TcpStream {
                local_addr: con.local_addr,
                peer_addr: con.peer_addr,
            })
        } else {
            Err(Error::new(
                ErrorKind::Other,
                "Simulation context has dropped TcpListener",
            ))
        }
    }

    pub(self) fn tcp_bind_stream(&mut self, peer: SocketAddr) -> Result<TcpStream> {
        //TODO Check peer validity
        let addr = self.tcp_bind_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))?;

        let buf = TcpStreamHandle {
            local_addr: addr,
            peer_addr: peer,

            acked: false,
            connection_failed: false,

            incoming: Vec::new(),
            interests: Vec::new(),

            config: TcpSocketConfig::stream(addr),
        };

        self.tcp_streams.insert((addr, peer), buf);

        return Ok(TcpStream {
            local_addr: addr,
            peer_addr: peer,
        });
    }

    // Finds and confirms an address.
    fn tcp_bind_addr(&mut self, mut addr: SocketAddr) -> Result<SocketAddr> {
        if addr.ip().is_unspecified() {
            // # Case 1: Unspecified address.

            // go for default interface;
            let mut intf = self
                .interfaces
                .iter()
                .map(|i| (i, i.prio))
                .collect::<Vec<_>>();

            intf.sort_by(|(_, lhs), (_, rhs)| lhs.cmp(rhs));

            // Iterate through
            for (interface, _) in intf {
                // Skip inactive ones
                if interface.status == InterfaceStatus::Inactive {
                    continue;
                }

                if !interface.flags.up {
                    continue;
                }

                // get a good addr
                for iaddr in &interface.addrs {
                    if let Some(next) = iaddr.next_ip() {
                        let mut addr = SocketAddr::new(next, addr.port());

                        if addr.port() == 0 {
                            // Grab the next one
                            addr.set_port(self.tcp_next_port);
                            self.tcp_next_port += 1;
                        } else {
                            // Check for collision
                            // TODO
                        }

                        return Ok(addr);
                    }
                }

                // WELP next interface
            }

            Err(Error::new(
                ErrorKind::AddrNotAvailable,
                "Address not available",
            ))
        } else {
            // # Case 2: Direct reference to a given interface

            // Check for sockets that allready have this key
            if self.tcp_listeners.get(&addr).is_some() {
                return Err(Error::new(ErrorKind::AddrInUse, "Address allready in use"));
            }

            // Find right interface
            for interface in &self.interfaces {
                if let Some(_iaddr) = interface
                    .addrs
                    .iter()
                    .find(|iaddr| iaddr.matches_ip(addr.ip()))
                {
                    // Found the right interface
                    if interface.status == InterfaceStatus::Inactive {
                        return Err(Error::new(ErrorKind::NotFound, "Interface inactive"));
                    }

                    if !interface.flags.up {
                        return Err(Error::new(ErrorKind::NotFound, "Interface down"));
                    }

                    // Ip Check now check for port number
                    if addr.port() == 0 {
                        // Grab the next one
                        addr.set_port(self.tcp_next_port);
                        self.tcp_next_port += 1;
                    }
                    // Else no collision possible
                    return Ok(addr);
                }
            }

            Err(Error::new(
                ErrorKind::AddrNotAvailable,
                "Address not available",
            ))
        }
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub(self) struct TcpListenerHandle {
    pub(super) local_addr: SocketAddr,

    pub(super) incoming: Vec<TcpListenerPendingConnection>,
    pub(self) config: TcpSocketConfig,
    pub(super) interests: Vec<IOInterestGuard>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(self) struct TcpListenerPendingConnection {
    pub(super) local_addr: SocketAddr,
    pub(super) peer_addr: SocketAddr,
}

/// A Handshake message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TcpConnectMessage {
    /// A initation from the client
    ///
    /// This message will be send by the client
    /// and should be received by the server
    ClientInitiate {
        /// The sender
        client: SocketAddr,
        /// The Receiver
        server: SocketAddr,
    },
    /// An Acknowledgement of an established connection
    ///
    /// This message will be send by the server.
    ServerAcknowledge {
        /// The Receiver
        client: SocketAddr,
        /// The Sender
        server: SocketAddr,
    },
}

impl TcpConnectMessage {
    /// The address this message should be send to.
    pub fn dest(&self) -> SocketAddr {
        match self {
            Self::ClientInitiate { server, .. } => *server,
            Self::ServerAcknowledge { client, .. } => *client,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub(self) struct TcpStreamHandle {
    pub(super) local_addr: SocketAddr,
    pub(super) peer_addr: SocketAddr,

    pub(super) acked: bool,
    pub(super) connection_failed: bool,

    pub(super) incoming: Vec<TcpMessage>,
    pub(self) config: TcpSocketConfig,
    pub(super) interests: Vec<IOInterestGuard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// A UDP Message in the network.
pub struct TcpMessage {
    /// The content byte-encoded.
    pub content: Vec<u8>,
    /// The senders bound address.
    pub src_addr: SocketAddr,
    /// The receivers address.
    pub dest_addr: SocketAddr,
    /// Time-To-Live
    pub ttl: u32,
}
