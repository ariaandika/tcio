//! Integration with [`tokio`] crate.
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead as TokioRead, AsyncWrite as TokioWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::bytes::{BufMut, UninitSlice};
use crate::io::{AsyncRead, AsyncWrite};

impl<'a> From<&'a mut UninitSlice> for ReadBuf<'a> {
    #[inline]
    fn from(value: &'a mut UninitSlice) -> Self {
        ReadBuf::uninit(unsafe { value.as_uninit_slice_mut() })
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        mut buf: impl BufMut,
        cx: &mut Context,
    ) -> Poll<io::Result<usize>> {
        let mut read = ReadBuf::from(buf.chunk_mut());
        ready!(TokioRead::poll_read(self, cx, &mut read))?;
        let read = read.filled().len();
        // SAFETY: `ReadBuf` guarantee that the filled is initialized
        unsafe { buf.advance_mut(read) };
        Poll::Ready(Ok(read))
    }
}

impl AsyncWrite for TcpStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, buf: &[u8], cx: &mut Context) -> Poll<io::Result<usize>> {
        TokioWrite::poll_write(self, cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        TokioWrite::poll_flush(self, cx)
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        TokioWrite::poll_shutdown(self, cx)
    }

    #[inline]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        bufs: &[IoSlice],
        cx: &mut Context,
    ) -> Poll<io::Result<usize>> {
        TokioWrite::poll_write_vectored(self, cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        TokioWrite::is_write_vectored(self)
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
        ) -> Poll<io::Result<usize>> {
            let mut read = ReadBuf::from(buf.chunk_mut());
            ready!(TokioRead::poll_read(self, cx, &mut read))?;
            let read = read.filled().len();
            // SAFETY: `ReadBuf` guarantee that the filled is initialized
            unsafe { buf.advance_mut(read) };
            Poll::Ready(Ok(read))
        }
    }

    impl AsyncWrite for UnixStream {
        #[inline]
        fn poll_write(
            self: Pin<&mut Self>,
            buf: &[u8],
            cx: &mut Context,
        ) -> Poll<io::Result<usize>> {
            TokioWrite::poll_write(self, cx, buf)
        }

        #[inline]
        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
            TokioWrite::poll_flush(self, cx)
        }

        #[inline]
        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
            TokioWrite::poll_shutdown(self, cx)
        }

        #[inline]
        fn poll_write_vectored(
            self: Pin<&mut Self>,
            bufs: &[IoSlice],
            cx: &mut Context,
        ) -> Poll<io::Result<usize>> {
            TokioWrite::poll_write_vectored(self, cx, bufs)
        }

        #[inline]
        fn is_write_vectored(&self) -> bool {
            TokioWrite::is_write_vectored(self)
        }
    }
}
