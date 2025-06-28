use std::{
    io,
    task::{Poll, ready},
};

use crate::io::AsyncBufRead;

/// Two layer cursor buffer reading.
#[derive(Debug)]
pub struct BufCursor<B> {
    io: B,
    read: usize,
}

impl<B> BufCursor<B> {
    /// Create new [`BufCursor`].
    #[inline]
    pub fn new(io: B) -> Self {
        Self { io, read: 0 }
    }

    /// Try get `len` of bytes, advancing cursor position.
    pub fn poll_get<'a>(&'a mut self, len: usize, cx: &mut std::task::Context) -> Poll<io::Result<&'a [u8]>>
    where
        B: AsyncBufRead,
    {
        loop {
            if self.io.chunk().len() >= self.read + len {
                let read = self.read;
                self.read += len;
                return Poll::Ready(Ok(&self.io.chunk()[read..read + len]))
            }

            ready!(self.io.poll_read_fill(cx)?);
        }
    }

    /// Set the underlying io buffer to consume amount of read by cursor, and reset cursor
    /// position.
    #[inline]
    pub fn commit(&mut self)
    where
        B: AsyncBufRead,
    {
        self.io.consume(self.read);
        self.read = 0;
    }
}


impl<B: AsyncBufRead> AsyncBufRead for BufCursor<B> {
    fn poll_read_fill(&mut self, cx: &mut std::task::Context) -> Poll<io::Result<usize>> {
        self.io.poll_read_fill(cx)
    }

    fn chunk(&self) -> &[u8] {
        &self.io.chunk()[self.read..]
    }

    fn consume(&mut self, cnt: usize) {
        self.read += cnt;
    }
}
