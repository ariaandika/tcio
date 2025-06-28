use std::{
    io,
    task::{Poll, ready},
};

/// Asynchronous io read operation.
pub trait AsyncIoRead {
    /// Polls for read readiness.
    fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>>;

    /// Tries to read data from the stream into the provided buffer, returning how many bytes were
    /// read.
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize>;

    /// Tries to read data from the stream into the provided buffers, returning how many bytes were
    /// read.
    fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize>;

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

    use tokio::net::TcpStream;

    impl AsyncIoRead for TcpStream {
        #[inline]
        fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
            self.poll_read_ready(cx)
        }

        #[inline]
        fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
            self.try_read(buf)
        }

        #[inline]
        fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
            self.try_read_vectored(bufs)
        }
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use tokio::net::UnixStream;

        impl AsyncIoRead for UnixStream {
            #[inline]
            fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
                self.poll_read_ready(cx)
            }

            #[inline]
            fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
                self.try_read(buf)
            }

            #[inline]
            fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
                self.try_read_vectored(bufs)
            }
        }
    }
}

