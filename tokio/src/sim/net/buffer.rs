use std::collections::VecDeque;

use crate::io::ReadBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SocketBuffer {
    buffers: VecDeque<PartialBuffer>,
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

impl SocketBuffer {
    pub(crate) fn new() -> SocketBuffer {
        Self {
            buffers: VecDeque::with_capacity(8),
        }
    }

    pub(crate) fn add(&mut self, buf: Vec<u8>) {
        self.buffers.push_back(PartialBuffer {
            buffer: buf,
            consumed: 0,
        });
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

            for i in start..(start + n) {
                buf[offset + i] = self.buffers[0].buffer[i]
            }

            self.buffers[0].consumed += n;
            if self.buffers[0].remaining() == 0 {
                let _ = self.buffers.pop_front();
            }
            required -= n;
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

            for i in start..(start + n) {
                buf[offset + i] = buffer.buffer[i]
            }

            required -= n;
            offset += n;
        }

        buf.len() - required
    }
}
