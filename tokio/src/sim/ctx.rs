use std::cell::RefCell;

thread_local! {
    static IOCTX: RefCell<SimContext> = const { RefCell::new(SimContext::empty()) }
}

/// The IO Contexxt
#[derive(Debug)]
pub struct SimContext {
    /// The IO Context
    #[cfg(feature = "net")]
    pub io: Option<IOContext>,

    /// The IO Context
    pub time: Option<TimeContext>,
}

impl SimContext {
    /// The intial sim_contex without subsequent entries.
    const fn empty() -> Self {
        Self {
            #[cfg(feature = "net")]
            io: None,

            time: None,
        }
    }

    /// Creates a new context
    #[cfg(feature = "net")]
    pub fn new(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            io: Some(IOContext::new(ether, v4)),
            time: None,
        }
    }

    /// Swaps out the current context
    pub fn swap(other: &mut SimContext) {
        IOCTX.with(|c| std::mem::swap(other, &mut *c.borrow_mut()))
    }
}

cfg_net! {
    use std::future::Future;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};
    use super::net::*;

    /// The IO Context
    #[derive(Debug)]
    pub struct IOContext {
        pub(crate) interfaces: Vec<Interface>,

        pub(crate) udp_sockets: Vec<UdpSocketBuffer>,
        pub(crate) udp_read_interest: Vec<ReadInterest>,
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
        pub fn yield_outgoing() -> Vec<UdpMessage> {
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
            IOCTX.with(|c| f(c.borrow_mut().io.as_mut().unwrap()))
        }
    }

    pub(crate) struct IOInterest {
        pub(crate) addr: SocketAddr,
    }

    impl Future for IOInterest {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            IOContext::with_current(|ctx| {
                if let Some(handle) = ctx
                    .udp_sockets
                    .iter()
                    .find(|s| *s.state.addr() == self.addr)
                {
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

    #[derive(Debug)]
    pub(crate) struct ReadInterest {
        addr: SocketAddr,
        waker: Waker,
    }

    #[derive(Debug)]
    pub(crate) struct UdpSocketBuffer {
        pub(crate) state: UdpSocketState,
        pub(crate) incoming: Vec<UdpMessage>,
        pub(crate) outgoing: Vec<UdpMessage>,
    }

    #[derive(Debug)]
    #[allow(unused)]
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

    impl fmt::Display for UdpSocketState {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Bound(ref addr) => write!(f, "LISTEN {}", addr),
                Self::Connected(ref local, ref target) => write!(f, "CONNCT {} {}", local, target)
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

}

cfg_time! {
    use super::time::{ SimTime};

    /// The time context
    #[derive(Debug)]
    pub struct TimeContext {
        #[allow(unused)]
        now: SimTime,

    }
}

use std::fmt;
impl fmt::Display for SimContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[SimContext]")?;

        #[cfg(feature = "net")]
        {
            if let Some(io) = self.io.as_ref() {
                writeln!(f, "[[net]]")?;
                for interface in &io.interfaces {
                    writeln!(
                        f,
                        "{:<6}  {}",
                        format!("{}:", interface.name),
                        interface.flags
                    )?;
                    for addr in &interface.addrs {
                        writeln!(f, "        {}", addr)?;
                    }
                    writeln!(f, "        status = {}", interface.status)?;
                }

                for socket_buffer in &io.udp_sockets {
                    writeln!(f, "UDP SOCKET {}", socket_buffer.state)?;
                    for msg in &socket_buffer.incoming {
                        writeln!(
                            f,
                            "  INCOMING {} {} [{} bytes]",
                            msg.from,
                            msg.to,
                            msg.buffer.len()
                        )?;
                    }
                    for msg in &socket_buffer.outgoing {
                        writeln!(
                            f,
                            "  OUTGOING {} {} [{} bytes]",
                            msg.from,
                            msg.to,
                            msg.buffer.len()
                        )?;
                    }
                }

                for interest in &io.udp_read_interest {
                    writeln!(f, "<{}>", interest.addr)?;
                }
            }
        }

        #[cfg(feature = "time")]
        {
            writeln!(f, "[[time]]")?;
        }

        Ok(())
    }
}
