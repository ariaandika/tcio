//! Integration with [`tokio`][<https://docs.rs/tokio>] crate.
use std::{
    io,
    pin::Pin,
    task::{Poll, ready},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, UnixStream},
};

/// Wrapper for tokio stream.
#[derive(Debug)]
pub struct IoStream {
    repr: Repr,
}

impl From<TcpStream> for IoStream {
    #[inline]
    fn from(value: TcpStream) -> Self {
        Self { repr: Repr::Tcp(value) }
    }
}

impl From<UnixStream> for IoStream {
    #[inline]
    fn from(value: UnixStream) -> Self {
        Self { repr: Repr::Unix(value) }
    }
}

#[derive(Debug)]
enum Repr {
    Tcp(TcpStream),
    Unix(UnixStream),
}

// ===== Readiness =====

impl IoStream {
    /// Polls for read readiness.
    ///
    /// For more details, see [`TcpStream::poll_read_ready`].
    #[inline]
    pub fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
        match &self.repr {
            Repr::Tcp(t) => t.poll_read_ready(cx),
            Repr::Unix(u) => u.poll_read_ready(cx),
        }
    }

    /// Polls for write readiness.
    ///
    /// For more details, see [`TcpStream::poll_write_ready`].
    #[inline]
    pub fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
        match &self.repr {
            Repr::Tcp(t) => t.poll_write_ready(cx),
            Repr::Unix(u) => u.poll_write_ready(cx),
        }
    }
}

// ===== IO =====

impl IoStream {
    /// Tries to read data from the stream into the provided buffer, returning how many bytes were
    /// read.
    ///
    /// For more details, see [`TcpStream::try_read`].
    #[inline]
    pub fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_read(buf),
            Repr::Unix(u) => u.try_read(buf),
        }
    }

    /// Tries to read data from the stream into the provided buffers, returning how many bytes were
    /// read.
    ///
    /// For more details, see [`TcpStream::try_read_vectored`].
    #[inline]
    pub fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_read_vectored(bufs),
            Repr::Unix(u) => u.try_read_vectored(bufs),
        }
    }

    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// For more details, see [`TcpStream::try_write`].
    #[inline]
    pub fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_write(buf),
            Repr::Unix(u) => u.try_write(buf),
        }
    }

    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// For more details, see [`TcpStream::try_write_vectored`].
    #[inline]
    pub fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_write_vectored(bufs),
            Repr::Unix(u) => u.try_write_vectored(bufs),
        }
    }
}

// ===== IO Buf Read =====

#[allow(clippy::missing_inline_in_public_items)]
impl IoStream {
    /// Tries to read data from the stream into the provided buffer, returning how many bytes were
    /// read.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for reading.
    pub fn poll_read(
        &self,
        buf: &mut [u8],
        cx: &mut std::task::Context,
    ) -> Poll<io::Result<usize>> {
        match self.try_read(buf) {
            Ok(read) => Poll::Ready(Ok(read)),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                tri!(ready!(self.poll_read_ready(cx)));
                self.poll_read(buf, cx)
            }
            Err(err) => Poll::Ready(Err(err)),
        }
    }

    /// Tries to read data from the stream into the provided buffer, advance buffer cursor,
    /// returning how many bytes were read.
    pub fn poll_read_buf<B>(
        &self,
        buf: &mut B,
        cx: &mut std::task::Context,
    ) -> Poll<io::Result<usize>>
    where
        B: bytes::BufMut + ?Sized,
    {
        if !buf.has_remaining_mut() {
            return Poll::Ready(Ok(0));
        }

        let read = {
            // SAFETY: we will only write initialized value and `MaybeUninit<T>` is guaranteed to
            // have the same size as `T`:
            let dst = unsafe {
                &mut *(buf.chunk_mut().as_uninit_slice_mut() as *mut [std::mem::MaybeUninit<u8>]
                    as *mut [u8])
            };

            tri!(ready!(self.poll_read(dst, cx)))
        };

        // SAFETY: This is guaranteed to be the number of initialized by `try_read`
        unsafe {
            buf.advance_mut(read);
        }

        Poll::Ready(Ok(read))
    }
}

// ===== IO Buf Write =====

#[allow(clippy::missing_inline_in_public_items)]
impl IoStream {
    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for writing.
    pub fn poll_write(&self, buf: &[u8], cx: &mut std::task::Context) -> Poll<io::Result<usize>> {
        match self.try_write(buf) {
            Ok(read) => Poll::Ready(Ok(read)),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                tri!(ready!(self.poll_write_ready(cx)));
                self.poll_write(buf, cx)
            }
            Err(err) => Poll::Ready(Err(err)),
        }
    }

    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for writing.
    pub fn poll_write_vectored(
        &self,
        bufs: &[io::IoSlice<'_>],
        cx: &mut std::task::Context,
    ) -> Poll<io::Result<usize>> {
        match self.try_write_vectored(bufs) {
            Ok(read) => Poll::Ready(Ok(read)),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                tri!(ready!(self.poll_write_ready(cx)));
                self.poll_write_vectored(bufs, cx)
            }
            Err(err) => Poll::Ready(Err(err)),
        }
    }

    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for writing.
    pub fn poll_write_buf<B>(
        &self,
        buf: &mut B,
        cx: &mut std::task::Context,
    ) -> Poll<io::Result<usize>>
    where
        B: bytes::Buf + ?Sized,
    {
        self.poll_write(buf.chunk(), cx)
            .map(|e| e.inspect(|&read| buf.advance(read)))
    }

    /// Tries to write all data from the provided buffer into the stream, advancing buffer cursor.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for writing.
    pub fn poll_write_all_buf<B>(&self, buf: &mut B) -> Poll<io::Result<()>>
    where
        B: bytes::Buf + ?Sized,
    {
        const MAX_VECTOR_ELEMENTS: usize = 64;

        while buf.has_remaining() {
            let read = if self.is_write_vectored() {
                let mut slices = [io::IoSlice::new(&[]); MAX_VECTOR_ELEMENTS];
                let cnt = buf.chunks_vectored(&mut slices);
                tri!(self.try_write_vectored(&slices[..cnt]))
            } else {
                tri!(self.try_write(buf.chunk()))
            };
            buf.advance(read);
            if read == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for IoStream {
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_read(cx, buf),
            Repr::Unix(u) => Pin::new(u).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for IoStream {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_write(cx, buf),
            Repr::Unix(u) => Pin::new(u).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_flush(cx),
            Repr::Unix(u) => Pin::new(u).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_shutdown(cx),
            Repr::Unix(u) => Pin::new(u).poll_shutdown(cx),
        }
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        match &self.repr {
            Repr::Tcp(t) => t.is_write_vectored(),
            Repr::Unix(u) => u.is_write_vectored(),
        }
    }

    #[inline]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_write_vectored(cx, bufs),
            Repr::Unix(u) => Pin::new(u).poll_write_vectored(cx, bufs),
        }
    }
}

// ===== Macros =====

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(ok) => ok,
            Err(err) => return Poll::Ready(Err(err)),
        }
    };
}

use tri;

