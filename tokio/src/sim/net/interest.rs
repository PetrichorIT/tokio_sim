use super::IOInterest;
use crate::io::{Interest, Ready};
use std::net::SocketAddr;

impl Interest {
    pub(super) fn udp_io_interest(self, socket: SocketAddr) -> (IOInterest, Ready) {
        if self.is_readable() {
            (IOInterest::UdpRead(socket), Ready::READABLE)
        } else if self.is_writable() {
            (IOInterest::UdpWrite(socket), Ready::WRITABLE)
        } else {
            todo!()
        }
    }

    pub(super) fn tcp_io_interest(
        self,
        socket: SocketAddr,
        peer: SocketAddr,
    ) -> (IOInterest, Ready) {
        if self.is_readable() {
            (IOInterest::TcpRead((socket, peer)), Ready::READABLE)
        } else if self.is_writable() {
            (IOInterest::TcpWrite((socket, peer)), Ready::WRITABLE)
        } else {
            todo!()
        }
    }
}
