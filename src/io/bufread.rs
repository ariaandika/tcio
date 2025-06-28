use std::{io, task::Poll};
use bytes::{Buf, BytesMut};

use crate::io::AsyncIoRead;

/// An [`AsyncIoRead`] with internal buffer.
///
/// The difference with tokio `AsyncBufRead` is that, the tokio version requires its internal
/// buffer to be empty to be able to read more data. Therefore, if the io buffer did not have
/// enough data, the receiver still also need an internal buffer of its own so that the io can read
/// more data.
///
/// This version did not requires io buffer to be empty to read more data, and receiver did not
/// requires its own internal buffer.
pub trait AsyncBufRead {
    /// Attempts to read from the underlying io into internal buffer, returning how many data read.
    fn poll_read_fill(&mut self, cx: &mut std::task::Context) -> Poll<io::Result<usize>>;

    /// Returns reference of internal buffer.
    fn chunk(&self) -> &[u8];

    /// Clear `cnt` data from the internal buffer.
    ///
    /// The `cnt` must be <= the number of bytes in the buffer returned
    /// [`chunk`][AsyncBufRead::chunk].
    fn consume(&mut self, cnt: usize);
}

/// An implementation of [`AsyncBufRead`] with given [`AsyncIoRead`].
#[derive(Debug)]
pub struct BufReader<IO> {
    io: IO,
    buf: BytesMut,
}

impl<IO> BufReader<IO> {
    /// Creates a new [`BufReader`].
    #[inline]
    pub fn new(io: IO) -> Self {
        Self { io, buf: BytesMut::new() }
    }

    /// Creates a new [`BufReader`] with the specified internal buffer capacity.
    #[inline]
    pub fn with_capacity(io: IO, capacity: usize) -> Self {
        Self { io, buf: BytesMut::with_capacity(capacity) }
    }
}

impl<IO> AsyncBufRead for BufReader<IO>
where
    IO: AsyncIoRead + Unpin
{
    #[inline]
    fn poll_read_fill(&mut self, cx: &mut std::task::Context) -> Poll<io::Result<usize>> {
        self.io.poll_read_buf(&mut self.buf, cx)
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        &self.buf
    }

    #[inline]
    fn consume(&mut self, cnt: usize) {
        self.buf.advance(cnt);
    }
}

