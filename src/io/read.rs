use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::bytes::BufMut;

/// Reads bytes asynchronously from a source.
///
/// This trait is analogous to the [`std::io::Read`] trait, but integrates with the asynchronous
/// task system. In particular, the [`poll_read`] method, unlike [`Read::read`], will automatically
/// queue the current task for wakeup and return if data is not yet available, rather than blocking
/// the calling thread.
///
/// Specifically, this means that the `poll_read` function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(()))` means that data was immediately read and placed into
///   the output buffer. The amount of data read can be determined by the
///   filled portion of the `BufMut`. If the difference is 0, either EOF has
///   been reached, or the output buffer had zero capacity
///   (i.e. `buf.remaining()` == 0).
///
/// * `Poll::Pending` means that no data was read into the buffer
///   provided. The I/O object is not currently readable but may become readable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is readable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   readable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `read` method only works in the context of a future's
/// task. The object may panic if used outside of a task.
///
/// [`poll_read`]: AsyncRead::poll_read
/// [`std::io::Read`]: std::io::Read
/// [`Read::read`]: std::io::Read::read
pub trait AsyncRead {
    /// Attempts to read from the `AsyncRead` into `buf`.
    ///
    /// On success, returns `Poll::Ready(Ok(()))` and places data in the unfilled portion of `buf`.
    /// If no data was read (`buf.filled().len()` is unchanged), it implies that EOF has been
    /// reached, or the output buffer had zero capacity (i.e. `buf.remaining() == 0`).
    ///
    /// If no data is available for reading, the method returns `Poll::Pending` and arranges for
    /// the current task (via `cx.waker()`) to receive a notification when the object becomes
    /// readable or is closed.
    fn poll_read(self: Pin<&mut Self>, buf: impl BufMut, cx: &mut Context) -> Poll<io::Result<()>>;
}

impl AsyncRead for &[u8] {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        mut buf: impl BufMut,
        _: &mut Context,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        let cnt = me.len().min(buf.remaining_mut());
        let (a, b) = me.split_at(cnt);
        buf.put_slice(a);
        *me = b;
        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for &mut T {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, buf: impl BufMut, cx: &mut Context) -> Poll<io::Result<()>> {
        T::poll_read(Pin::new(self.get_mut()), buf, cx)
    }
}

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for Box<T> {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, buf: impl BufMut, cx: &mut Context) -> Poll<io::Result<()>> {
        T::poll_read(Pin::new(self.get_mut()), buf, cx)
    }
}

impl<T> AsyncRead for Pin<T>
where
    T: std::ops::DerefMut,
    T::Target: AsyncRead,
{
    #[inline]
    fn poll_read(self: Pin<&mut Self>, buf: impl BufMut, cx: &mut Context) -> Poll<io::Result<()>> {
        T::Target::poll_read(Pin::as_deref_mut(self), buf, cx)
    }
}

// ===== tokio =====

#[cfg(feature = "tokio")]
mod tokio_io {
    use super::*;

    use tokio::io::{AsyncRead as TokioRead, ReadBuf};
    use tokio::net::TcpStream;

    impl AsyncRead for TcpStream {
        fn poll_read(
            self: Pin<&mut Self>,
            mut buf: impl BufMut,
            cx: &mut Context,
        ) -> Poll<io::Result<()>> {
            let mut read = ReadBuf::from(buf.chunk_mut());
            let result = TokioRead::poll_read(self, cx, &mut read);
            let read = read.filled().len();
            // SAFETY: `ReadBuf` guarantee that the filled is initialized
            unsafe { buf.advance_mut(read) };
            result
        }
    }

    #[cfg(unix)]
    mod unix {
        use super::*;
        use tokio::net::UnixStream;

        impl AsyncRead for UnixStream {
            fn poll_read(
                self: Pin<&mut Self>,
                mut buf: impl BufMut,
                cx: &mut Context,
            ) -> Poll<io::Result<()>> {
                let mut read = ReadBuf::from(buf.chunk_mut());
                let result = TokioRead::poll_read(self, cx, &mut read);
                let read = read.filled().len();
                // SAFETY: `ReadBuf` guarantee that the filled is initialized
                unsafe { buf.advance_mut(read) };
                result
            }
        }
    }
}
