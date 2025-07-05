use std::{
    io,
    task::{Poll, ready},
};

/// Asynchronous io write operation.
pub trait AsyncIoWrite {
    /// Polls for write readiness.
    fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>>;

    /// Try to write a buffer to the stream, returning how many bytes were written.
    fn try_write(&self, buf: &[u8]) -> io::Result<usize>;

    /// Try to write a buffer to the stream, returning how many bytes were written.
    fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize>;

    /// Returns `true` if the underlying io support vectored write.
    fn is_write_vectored(&self) -> bool;

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
    fn poll_write_all_buf<B>(&self, buf: &mut B, cx: &mut std::task::Context) -> Poll<io::Result<()>>
    where
        B: bytes::Buf + ?Sized,
    {
        const MAX_VECTOR_ELEMENTS: usize = 64;

        while buf.has_remaining() {
            let read = if self.is_write_vectored() {
                let mut slices = [io::IoSlice::new(&[]); MAX_VECTOR_ELEMENTS];
                let cnt = buf.chunks_vectored(&mut slices);
                tri!(ready!(self.poll_write_vectored(&slices[..cnt], cx)))
            } else {
                tri!(ready!(self.poll_write(buf.chunk(), cx)))
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

// ===== tokio =====

#[cfg(feature = "tokio")]
mod tokio_io {
    use super::*;

    use tokio::{io::AsyncWrite, net::TcpStream};

    impl AsyncIoWrite for TcpStream {
        #[inline]
        fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_write_ready(cx)
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

    #[cfg(unix)]
    mod unix {
        use super::*;
        use tokio::net::UnixStream;

        impl AsyncIoWrite for UnixStream {
            #[inline]
            fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
                self.poll_write_ready(cx)
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
}


