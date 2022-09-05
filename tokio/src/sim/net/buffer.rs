use crate::io::ReadBuf;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SocketIncomingBuffer {
    buffers: VecDeque<PartialBuffer>,
    len: usize,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartialBuffer {
    buffer: Vec<u8>,
    consumed: usize,
}

impl PartialBuffer {
    fn remaining(&self) -> usize {
        self.buffer.len() - self.consumed
    }
}

impl SocketIncomingBuffer {
    pub(crate) fn new(limit: u32) -> SocketIncomingBuffer {
        Self {
            buffers: VecDeque::with_capacity(8),
            len: 0,
            limit: limit as usize,
        }
    }

    pub(crate) fn add(&mut self, buf: Vec<u8>) {
        self.len += buf.len();
        if self.len > self.limit {
            // TODO: Temprarily this will not be active but soon
            eprintln!("Recv buffer size exceeded");
            self.buffers.push_back(PartialBuffer {
                buffer: buf,
                consumed: 0,
            });
        } else {
            self.buffers.push_back(PartialBuffer {
                buffer: buf,
                consumed: 0,
            });
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Returns the number of bytes still missing
    pub(crate) fn read_buf(&mut self, buf: &mut ReadBuf<'_>) -> usize {
        let mut required = buf.remaining();

        while !self.buffers.is_empty() && required > 0 {
            let n = required.min(self.buffers[0].remaining());
            let start = self.buffers[0].consumed;

            buf.put_slice(&self.buffers[0].buffer[start..(start + n)]);
            self.buffers[0].consumed += n;
            if self.buffers[0].remaining() == 0 {
                let _ = self.buffers.pop_front();
            }

            self.len -= n;
            required -= n;
        }

        required
    }

    /// Returns the number of bytes read
    pub(crate) fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut required = buf.len();
        let mut offset = 0;

        while !self.buffers.is_empty() && required > 0 {
            let n = required.min(self.buffers[0].remaining());
            let start = self.buffers[0].consumed;

            for i in 0..n {
                buf[offset + i] = self.buffers[0].buffer[start + i]
            }

            self.buffers[0].consumed += n;
            if self.buffers[0].remaining() == 0 {
                let _ = self.buffers.pop_front();
            }
            required -= n;
            self.len -= n;
            offset += n;
        }

        buf.len() - required
    }

    pub(crate) fn peek(&mut self, buf: &mut [u8]) -> usize {
        let mut required = buf.len();
        let mut offset = 0;

        for buffer in &self.buffers {
            if required == 0 {
                return buf.len();
            }

            let n = required.min(buffer.remaining());
            let start = buffer.consumed;

            for i in 0..n {
                buf[offset + i] = buffer.buffer[start + i]
            }

            required -= n;
            offset += n;
        }

        buf.len() - required
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SocketOutgoingBuffer {
    packets: Vec<Vec<u8>>,
    len: usize,
    limit: usize,
}

impl SocketOutgoingBuffer {
    pub(crate) fn new(limit: u32) -> SocketOutgoingBuffer {
        SocketOutgoingBuffer {
            packets: Vec::new(),
            len: 0,
            limit: limit as usize,
        }
    }

    pub(crate) fn write<'a: 'b, 'b>(&mut self, mut buf: &'a [u8]) -> Result<(), &'b [u8]> {
        while buf.len() > 0 && self.limit > self.len {
            let pkt = if let Some(pkt) = self.packets.last_mut() {
                if pkt.len() < 1024 {
                    pkt
                } else {
                    self.packets.push(Vec::with_capacity(1024));
                    self.packets.last_mut().unwrap()
                }
            } else {
                self.packets.push(Vec::with_capacity(1024));
                self.packets.last_mut().unwrap()
            };

            // the number of bytes to be written
            let n = buf.len().min(1024 - pkt.len()).min(self.limit - self.len);
            let offset = pkt.len();

            // Extended buffer
            self.len += n;
            unsafe { pkt.set_len(offset + n) };
            pkt[offset..(offset + n)].copy_from_slice(&buf[..n]);

            buf = &buf[n..]
        }

        if buf.len() == 0 {
            Ok(())
        } else {
            Err(buf)
        }
    }

    pub(crate) fn yield_packets(&mut self) -> Vec<Vec<u8>> {
        self.len = 0;

        let mut swap = Vec::new();
        std::mem::swap(&mut swap, &mut self.packets);
        swap
    }
}
