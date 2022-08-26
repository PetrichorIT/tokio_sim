use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

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

    pub(crate) prio: usize,
}

impl Interface {
    /// Creates a loopback interface
    pub fn loopback() -> Self {
        Self {
            name: "lo0".to_string(),
            flags: InterfaceFlags::loopback(),
            addrs: Vec::from(InterfaceAddr::loopback()),
            status: InterfaceStatus::Active,
            prio: 100,
        }
    }

    /// Creates a loopback interface
    pub fn en0(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            name: "en0".to_string(),
            flags: InterfaceFlags::en0(),
            addrs: Vec::from(InterfaceAddr::en0(ether, v4)),
            status: InterfaceStatus::Active,
            prio: 10,
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
    pub multicast: bool,
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
            multicast: true,
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
            multicast: true,
            p2p: false,
            broadcast: true,
            smart: true,
            simplex: true,
            promisc: false,
        }
    }
}

impl fmt::Display for InterfaceFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "flags=<")?;
        if self.up {
            write!(f, "UP")?
        }
        if self.loopback {
            write!(f, "LOOPBACK")?
        }
        if self.running {
            write!(f, "RUNNING")?
        }
        if self.multicast {
            write!(f, "MULTICAST")?
        }
        if self.p2p {
            write!(f, "POINTTOPOINT")?
        }
        if self.broadcast {
            write!(f, "BROADCAST")?
        }
        if self.smart {
            write!(f, "SMART")?
        }
        if self.simplex {
            write!(f, "SIMPLEX")?
        }
        if self.promisc {
            write!(f, "PROMISC")?
        }

        write!(f, ">")
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

impl fmt::Display for InterfaceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
        }
    }
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

    /// Indicates whether the given ip is valid on the interface address.
    pub fn matches_ip(&self, ip: IpAddr) -> bool {
        match self {
            Self::Inet { addr, netmask } if ip.is_ipv4() => {
                let ip = if let IpAddr::V4(v) = ip {
                    v
                } else {
                    unreachable!()
                };

                let ip_u32 = u32::from_be_bytes(ip.octets());
                let addr_u32 = u32::from_be_bytes(addr.octets());
                let mask_u32 = u32::from_be_bytes(netmask.octets());

                mask_u32 & ip_u32 == addr_u32
            }
            Self::Inet6 { .. } if ip.is_ipv6() => {
                todo!()
            }
            _ => false,
        }
    }

    /// Returns an available Ip.
    pub fn next_ip(&self) -> Option<IpAddr> {
        match self {
            Self::Ether { .. } => None,
            Self::Inet { addr, .. } => Some(IpAddr::V4(*addr)),
            Self::Inet6 { addr, .. } => Some(IpAddr::V6(*addr)),
        }
    }
}

impl fmt::Display for InterfaceAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Ether { addr } => write!(
                f,
                "ether {}:{}:{}:{}:{}:{}",
                addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]
            ),
            Self::Inet { addr, netmask } => write!(f, "inet {} netmask {}", addr, netmask),
            Self::Inet6 {
                addr,
                prefixlen,
                scope_id,
            } => write!(
                f,
                "inet6 {} prefixlen {}{}",
                addr,
                prefixlen,
                if let Some(scope_id) = scope_id {
                    format!(" scopeid: 0x{:x}", scope_id)
                } else {
                    String::new()
                }
            ),
        }
    }
}
