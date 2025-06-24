//! Asynchronous io.
#![allow(clippy::missing_inline_in_public_items)]
use std::{
    io,
    task::{Poll, ready},
};

/// Asynchronous io operation.
pub trait AsyncIo {
    /// Polls for read readiness.
    fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>>;

    /// Polls for write readiness.
    fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>>;

    /// Tries to read data from the stream into the provided buffer, returning how many bytes were
    /// read.
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize>;

    /// Tries to read data from the stream into the provided buffers, returning how many bytes were
    /// read.
    fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize>;

    /// Try to write a buffer to the stream, returning how many bytes were written.
    fn try_write(&self, buf: &[u8]) -> io::Result<usize>;

    /// Try to write a buffer to the stream, returning how many bytes were written.
    fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize>;

    /// Returns `true` if the underlying io support vectored write.
    fn is_write_vectored(&self) -> bool;

    // ===== IO Read =====

    /// Tries to read data from the stream into the provided buffer, returning how many bytes were
    /// read.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for reading.
    fn poll_read(
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
    fn poll_read_buf<B>(
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

    // ===== IO Write =====

    /// Try to write a buffer to the stream, returning how many bytes were written.
    ///
    /// Returns [`Poll::Pending`] if the underlying stream not ready for writing.
    fn poll_write(&self, buf: &[u8], cx: &mut std::task::Context) -> Poll<io::Result<usize>> {
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
    fn poll_write_vectored(
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
    fn poll_write_buf<B>(
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
    fn poll_write_all_buf<B>(&self, buf: &mut B) -> Poll<io::Result<()>>
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

#[cfg(feature = "tokio")]
mod tokio_io {
    use super::*;

    use tokio::{
        io::AsyncWrite,
        net::{TcpStream, UnixStream},
    };

    impl AsyncIo for TcpStream {
        #[inline]
        fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_read_ready(cx)
        }

        #[inline]
        fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_write_ready(cx)
        }

        #[inline]
        fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
            self.try_read(buf)
        }

        #[inline]
        fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
            self.try_read_vectored(bufs)
        }

        #[inline]
        fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
            self.try_write(buf)
        }

        #[inline]
        fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
            self.try_write_vectored(bufs)
        }

        #[inline]
        fn is_write_vectored(&self) -> bool {
            AsyncWrite::is_write_vectored(self)
        }
    }

    impl AsyncIo for UnixStream {
        #[inline]
        fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_read_ready(cx)
        }

        #[inline]
        fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_write_ready(cx)
        }

        #[inline]
        fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
            self.try_read(buf)
        }

        #[inline]
        fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
            self.try_read_vectored(bufs)
        }

        #[inline]
        fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
            self.try_write(buf)
        }

        #[inline]
        fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
            self.try_write_vectored(bufs)
        }

        #[inline]
        fn is_write_vectored(&self) -> bool {
            AsyncWrite::is_write_vectored(self)
        }
    }
}

