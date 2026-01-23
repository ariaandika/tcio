use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::bytes::BufMut;

/// Read bytes asynchronously.
///
/// This trait is analogous to the `std::io::Read` trait, but integrates with the asynchronous task
/// system. In particular, the `poll_read` method, unlike `Read::read`, will automatically queue
/// the current task for wakeup and return if data is not yet available, rather than blocking the
/// calling thread.
pub trait AsyncRead {
    /// Attempt to read from the `AsyncRead` into `buf`.
    ///
    /// On success, returns `Poll::Ready(Ok(num_bytes_read))`. If `n` is `0`,  it implies that EOF
    /// has been reached, or the output buffer had zero capacity.
    ///
    /// Note that callers does not need to advance `buf` after the call, implementor is the one
    /// responsible for that. The returned `num_bytes_read` can be used by caller to detect a zero
    /// length read call.
    ///
    /// If no data is available for reading, the method returns `Poll::Pending` and arranges for
    /// the current task (via `cx.waker()`) to receive a notification when the object becomes
    /// readable or is closed.
    fn poll_read(self: Pin<&mut Self>, buf: impl BufMut, cx: &mut Context) -> Poll<io::Result<usize>>;
}

impl AsyncRead for &[u8] {
    fn poll_read(
        self: Pin<&mut Self>,
        mut buf: impl BufMut,
        _: &mut Context,
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        let cnt = me.len().min(buf.remaining_mut());
        let (read, rest) = me.split_at(cnt);
        buf.put_slice(read);
        *me = rest;
        Poll::Ready(Ok(cnt))
    }
}

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for &mut T {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        buf: impl BufMut,
        cx: &mut Context,
    ) -> Poll<io::Result<usize>> {
        T::poll_read(Pin::new(self.get_mut()), buf, cx)
    }
}

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for Box<T> {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        buf: impl BufMut,
        cx: &mut Context,
    ) -> Poll<io::Result<usize>> {
        T::poll_read(Pin::new(self.get_mut()), buf, cx)
    }
}

impl<T> AsyncRead for Pin<T>
where
    T: std::ops::DerefMut,
    T::Target: AsyncRead,
{
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        buf: impl BufMut,
        cx: &mut Context,
    ) -> Poll<io::Result<usize>> {
        T::Target::poll_read(Pin::as_deref_mut(self), buf, cx)
    }
}
