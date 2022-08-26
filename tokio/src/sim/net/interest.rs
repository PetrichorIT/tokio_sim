use super::IOInterest;
use crate::io::Ready;
use std::net::SocketAddr;
use std::ops;

/// Readiness event interest.
///
/// Specifies the readiness events the caller is interested in when awaiting on I/O resource readiness states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interest {
    pub(super) id: usize,
}

// [W-Bit, R-Bit]
impl Interest {
    /// Interest in all readable events.
    pub const READABLE: Interest = Interest { id: 0b1 };

    /// Interest in all writable events.
    pub const WRITABLE: Interest = Interest { id: 0b10 };

    /// Returns true if the value includes readable interest.
    pub const fn is_readable(&self) -> bool {
        self.id & 0b1 != 0
    }

    /// Returns true if the value includes writable  interest.
    pub const fn is_writable(&self) -> bool {
        self.id & 0b01 != 0
    }

    /// Add together two Interest values.
    pub const fn add(self, other: Interest) -> Interest {
        Interest {
            id: self.id | other.id,
        }
    }

    pub(super) fn io_interest(self, socket: SocketAddr, is_tcp: bool) -> (IOInterest, Ready) {
        match self.id {
            0b1 if !is_tcp => (IOInterest::UdpRead(socket), Ready::READABLE),
            0b01 if !is_tcp => (IOInterest::UdpWrite(socket), Ready::WRITABLE),
            _ => todo!(),
        }
    }
}

impl ops::BitOr<Interest> for Interest {
    type Output = Interest;

    fn bitor(self, other: Interest) -> Interest {
        self.add(other)
    }
}

impl ops::BitOrAssign for Interest {
    fn bitor_assign(&mut self, other: Self) {
        *self = self.add(other)
    }
}
