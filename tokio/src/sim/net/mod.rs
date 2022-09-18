use super::time::SimTime;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

pub(crate) use std::io::Result;

/// Network interfaces.
pub mod interface;
use interface::*;

pub mod unix;
pub mod windows;

mod addr;
pub use addr::*;

mod buffer;
use buffer::SocketIncomingBuffer;
use buffer::SocketOutgoingBuffer;

mod lookup_host;
pub use lookup_host::*;

mod udp;
pub use udp::*;

/// TCP utility types.
pub mod tcp;
use tcp::{TcpSocketConfig, TcpStreamInner};

pub use tcp::listener::TcpListener;
pub use tcp::socket::TcpSocket;
pub use tcp::stream::TcpStream;

mod interest;
pub use interest::*;

/// Gets the mac address.
pub fn get_mac_address() -> Result<Option<[u8; 6]>> {
    IOContext::try_with_current(|ctx| ctx.get_mac_address())
        .unwrap_or(Err(Error::new(ErrorKind::Other, "No SimContext bound")))
}

/// Gets the ip addr
pub fn get_ip() -> Option<IpAddr> {
    IOContext::with_current(|ctx| ctx.get_ip())
}

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
    ///
    /// Contains the message and the intented send delay.
    TcpSendPacket(TcpMessage, Duration),

    /// A indication that the context whould be reactivied once the current workload
    /// was processed (in the next tick).
    ///
    /// The provided parameter acts as a lower bound for the io tick.
    /// it must be smaller that the chossen delay, or the tick should be discarded,
    IoTick(SimTime),

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

    pub(self) tick_wakeups: Vec<Waker>,
    pub(self) next_io_tick: SimTime,
}

impl IOContext {
    /// An empty IO Context, just a dummy
    pub fn empty() -> Self {
        Self {
            interfaces: Vec::new(),
            intents: Vec::new(),

            udp_sockets: HashMap::new(),
            tcp_listeners: HashMap::new(),
            tcp_streams: HashMap::new(),
            tcp_next_port: 0,

            tick_wakeups: Vec::new(),
            next_io_tick: SimTime::MIN,
        }
    }

    /// Creates a new IO Context.
    pub fn new(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            interfaces: vec![Interface::loopback(), Interface::en0(ether, v4)],

            intents: Vec::new(),

            udp_sockets: HashMap::new(),
            tcp_listeners: HashMap::new(),
            tcp_streams: HashMap::new(),
            tcp_next_port: 1024,

            tick_wakeups: Vec::new(),
            next_io_tick: SimTime::MIN,
        }
    }

    /// Sets the IO Context
    pub fn set(self) {
        use super::ctx::IOCTX;
        IOCTX.with(|c| c.borrow_mut().io = Some(self))
    }

    /// Returns the mac address of the given IO Context.
    pub fn get_mac_address(&mut self) -> Result<Option<[u8; 6]>> {
        for interface in &self.interfaces {
            for addr in &interface.addrs {
                if let InterfaceAddr::Ether { addr } = addr {
                    return Ok(Some(*addr));
                }
            }
        }
        Ok(None)
    }

    /// Returns the ip of the given IO Context
    pub fn get_ip(&mut self) -> Option<IpAddr> {
        let bind = self.bind_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
        match bind {
            Ok(v) => Some(v.ip()),
            Err(_) => None,
        }
    }

    ///
    /// Resets the context after a module restart.
    ///
    pub(super) fn reset(&mut self) {
        self.intents.clear();
        self.udp_sockets.clear();
        self.tcp_listeners.clear();
        self.tcp_streams.clear();
        self.tcp_next_port = 1024;
    }

    pub(crate) fn with_current<R>(f: impl FnOnce(&mut IOContext) -> R) -> R {
        use super::ctx::IOCTX;
        IOCTX.with(|c| f(c.borrow_mut().io.as_mut().expect("Missing IO Context")))
    }

    pub(crate) fn try_with_current<R>(f: impl FnOnce(&mut IOContext) -> R) -> Option<R> {
        use super::ctx::IOCTX;
        IOCTX.with(|c| Some(f(c.borrow_mut().io.as_mut()?)))
    }

    pub(crate) fn yield_intents(&mut self) -> Vec<IOIntent> {
        let mut swap = Vec::new();
        std::mem::swap(&mut swap, &mut self.intents);

        // # TCP message creation
        let mut delay = Duration::ZERO;

        for (_, handle) in self.tcp_streams.iter_mut() {
            for packet in handle.outgoing.yield_packets() {
                swap.push(IOIntent::TcpSendPacket(
                    TcpMessage {
                        content: packet,
                        ttl: handle.config.ttl,
                        dest_addr: handle.peer_addr,
                        src_addr: handle.local_addr,
                    },
                    delay,
                ));

                delay += Duration::from_millis(5);
            }
        }

        // # Check for IoTick
        let tick_time = SimTime::now() + delay;
        if !self.tick_wakeups.is_empty() && tick_time > self.next_io_tick {
            swap.push(IOIntent::IoTick(tick_time));
            self.next_io_tick = tick_time;
        }

        swap
    }

    pub(crate) fn io_tick(&mut self) {
        self.tick_wakeups.drain(..).for_each(|w| w.wake());
        self.next_io_tick = SimTime::MIN;
    }

    ///
    /// Processes a UDP packet.
    ///
    pub(crate) fn process_udp(&mut self, msg: UdpMessage) -> std::result::Result<(), UdpMessage> {
        let sock = msg.dest_addr;

        match msg.dest_addr.ip() {
            IpAddr::V4(ip) if ip.is_broadcast() => {
                // all socket received
                let mut recv = false;
                for (_, handle) in self
                    .udp_sockets
                    .iter_mut()
                    .filter(|(k, _)| k.port() == msg.dest_addr.port())
                {
                    handle.incoming.push_back(msg.clone());
                    handle.interests.drain(..).for_each(|w| w.waker.wake());
                    recv = true
                }
                if recv {
                    Ok(())
                } else {
                    Err(msg)
                }
            }
            _ => {
                if let Some(handle) = self.udp_sockets.get_mut(&sock) {
                    handle.incoming.push_back(msg);
                    handle.interests.drain(..).for_each(|w| w.waker.wake());
                    Ok(())
                } else {
                    println!("Dropping UDP Message :  {:?}", msg);
                    Err(msg)
                }
            }
        }
    }

    ///
    /// Processes a TCP Connection Message.
    ///
    pub(crate) fn process_tcp_connect(
        &mut self,
        msg: TcpConnectMessage,
    ) -> std::result::Result<(), TcpConnectMessage> {
        match msg {
            // Server side code
            TcpConnectMessage::ClientInitiate { client, server } => {
                // look for listener
                if let Some(handle) = self.tcp_listeners.get_mut(&server) {
                    handle.incoming.push_back(TcpListenerPendingConnection {
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
                        }));
                    Ok(())
                } else {
                    Err(msg)
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
                    Ok(())
                } else {
                    Err(msg)
                }
            }
        }
    }

    ///
    /// Processa a tcp packet
    ///
    pub(crate) fn process_tcp_packet(
        &mut self,
        msg: TcpMessage,
    ) -> std::result::Result<(), TcpMessage> {
        if let Some(handle) = self.tcp_streams.get_mut(&(msg.dest_addr, msg.src_addr)) {
            handle.incoming.add(msg.content);

            let mut i = 0;
            while i < handle.interests.len() {
                if matches!(handle.interests[i].interest, IOInterest::TcpRead(_)) {
                    let w = handle.interests.swap_remove(i);
                    w.waker.wake();
                } else {
                    i += 1;
                }
            }
            Ok(())
        } else {
            Err(msg)
        }
    }

    ///
    /// Processes a timeout
    ///
    pub(crate) fn process_tcp_connect_timeout(
        &mut self,
        msg: TcpConnectMessage,
    ) -> std::result::Result<(), TcpConnectMessage> {
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
                    Ok(())
                } else {
                    Err(msg)
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
        let addr = self.bind_addr(addr)?;

        let buf = UdpSocketHandle {
            local_addr: addr,
            state: UdpSocketState::Bound,
            incoming: VecDeque::new(),

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

    pub(self) fn udp_send(
        &mut self,
        src_addr: SocketAddr,
        dest_addr: SocketAddr,
        content: Vec<u8>,
    ) -> Result<()> {
        // (1.1) Check a socket exits
        let handle = match self.udp_sockets.get(&src_addr) {
            Some(v) => v,
            None => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Simulation context has dropped UdpSocket",
                ))
            }
        };

        // (1.2) Check Broadcast
        if let IpAddr::V4(dest_addr) = dest_addr.ip() {
            if dest_addr.is_broadcast() && !handle.broadcast {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Cannot send broadcast without broadcast flag enabled",
                ));
            }
        }

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
    pub(super) incoming: VecDeque<UdpMessage>,

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
    pub(self) fn tcp_bind_listener(
        &mut self,
        addr: SocketAddr,
        config: Option<TcpSocketConfig>,
    ) -> Result<TcpListener> {
        let addr = self.bind_addr(addr)?;

        let buf = TcpListenerHandle {
            local_addr: addr,
            incoming: VecDeque::new(),
            interests: Vec::new(),

            config: config.unwrap_or(TcpSocketConfig::listener(addr)),
        };

        self.tcp_listeners.insert(addr, buf);

        return Ok(TcpListener { addr });
    }

    pub(self) fn tcp_drop_listener(&mut self, addr: SocketAddr) {
        self.tcp_listeners.remove(&addr);
    }

    pub(self) fn tcp_accept(&mut self, addr: SocketAddr) -> Result<TcpStream> {
        if let Some(handle) = self.tcp_listeners.get_mut(&addr) {
            let con = match handle.incoming.pop_front() {
                Some(con) => con,
                None => return Err(Error::new(ErrorKind::WouldBlock, "WouldBlock")),
            };

            assert_eq!(con.local_addr, addr);

            let config = handle.config.accept(con);

            let buf = TcpStreamHandle {
                local_addr: con.local_addr,
                peer_addr: con.peer_addr,

                acked: true,
                connection_failed: false,

                incoming: SocketIncomingBuffer::new(config.recv_buffer_size),
                interests: Vec::new(),
                outgoing: SocketOutgoingBuffer::new(config.send_buffer_size),

                config,
            };
            self.tcp_streams
                .insert((con.local_addr, con.peer_addr), buf);
            Ok(TcpStream {
                inner: Arc::new(TcpStreamInner {
                    local_addr: con.local_addr,
                    peer_addr: con.peer_addr,
                }),
            })
        } else {
            Err(Error::new(
                ErrorKind::Other,
                "Simulation context has dropped TcpListener",
            ))
        }
    }

    pub(self) fn tcp_bind_stream(
        &mut self,
        peer: SocketAddr,
        config: Option<TcpSocketConfig>,
    ) -> Result<TcpStream> {
        //TODO Check peer validity
        let addr = self.bind_addr(
            config
                .as_ref()
                .map(|c| c.addr)
                .unwrap_or(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
        )?;

        let config = config.unwrap_or(TcpSocketConfig::stream(addr));

        let buf = TcpStreamHandle {
            local_addr: addr,
            peer_addr: peer,

            acked: false,
            connection_failed: false,

            incoming: SocketIncomingBuffer::new(config.recv_buffer_size),
            interests: Vec::new(),
            outgoing: SocketOutgoingBuffer::new(config.send_buffer_size),

            config,
        };

        self.tcp_streams.insert((addr, peer), buf);

        return Ok(TcpStream {
            inner: Arc::new(TcpStreamInner {
                local_addr: addr,
                peer_addr: peer,
            }),
        });
    }
}

impl IOContext {
    // Finds and confirms an address.
    fn bind_addr(&mut self, mut addr: SocketAddr) -> Result<SocketAddr> {
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
                            loop {
                                addr.set_port(self.tcp_next_port);
                                self.tcp_next_port += 1;

                                if self.tcp_listeners.get(&addr).is_none() {
                                    break;
                                }
                            }
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
                        loop {
                            addr.set_port(self.tcp_next_port);
                            self.tcp_next_port += 1;

                            if self.tcp_listeners.get(&addr).is_none() {
                                break;
                            }
                        }
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

    pub(super) incoming: VecDeque<TcpListenerPendingConnection>,
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

    pub(super) incoming: SocketIncomingBuffer,
    pub(super) interests: Vec<IOInterestGuard>,
    pub(super) outgoing: SocketOutgoingBuffer,

    pub(self) config: TcpSocketConfig,
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
