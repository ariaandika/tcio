use std::{io, pin::Pin, task::Poll};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpStream, ToSocketAddrs},
};

#[cfg(unix)]
use tokio::net::UnixStream;

use crate::{
    futures::map,
    io::{AsyncIoRead, AsyncIoWrite},
};

/// Wrapper for either tokio [`TcpStream`] or [`UnixStream`][tokio::net::UnixStream].
///
/// IO Operation provided via [`AsyncIo`].
pub struct IoStream {
    repr: Repr,
}

enum Repr {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl std::fmt::Debug for IoStream {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.repr {
            Repr::Tcp(t) => f.debug_tuple("IoStream").field(t).finish(),
            Repr::Unix(u) => f.debug_tuple("IoStream").field(u).finish(),
        }
    }
}

impl IoStream {
    /// Opens a TCP connection to a remote host.
    #[inline]
    pub fn connect<A>(addr: A) -> impl Future<Output = io::Result<Self>>
    where
        A: ToSocketAddrs,
    {
        map(TcpStream::connect(addr), |e| match e {
            Ok(ok) => Ok(Self {
                repr: Repr::Tcp(ok),
            }),
            Err(err) => Err(err),
        })
    }

    /// Connects to the unix socket named by `path`.
    #[cfg(unix)]
    #[inline]
    pub fn connect_unix<P>(path: P) -> impl Future<Output = io::Result<Self>>
    where
        P: AsRef<std::path::Path>,
    {
        map(UnixStream::connect(path), |e| match e {
            Ok(ok) => Ok(Self {
                repr: Repr::Unix(ok),
            }),
            Err(err) => Err(err),
        })
    }
}

impl From<TcpStream> for IoStream {
    #[inline]
    fn from(value: TcpStream) -> Self {
        Self { repr: Repr::Tcp(value) }
    }
}

#[cfg(unix)]
impl From<UnixStream> for IoStream {
    #[inline]
    fn from(value: UnixStream) -> Self {
        Self { repr: Repr::Unix(value) }
    }
}

// ===== AsyncIo =====

impl AsyncIoRead for IoStream {
    #[inline]
    fn poll_read_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
        match &self.repr {
            Repr::Tcp(t) => t.poll_read_ready(cx),
            #[cfg(unix)]
            Repr::Unix(u) => u.poll_read_ready(cx),
        }
    }

    #[inline]
    fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_read(buf),
            #[cfg(unix)]
            Repr::Unix(u) => u.try_read(buf),
        }
    }

    #[inline]
    fn try_read_vectored(&self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_read_vectored(bufs),
            #[cfg(unix)]
            Repr::Unix(u) => u.try_read_vectored(bufs),
        }
    }
}

impl AsyncIoWrite for IoStream {
    #[inline]
     fn poll_write_ready(&self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
        match &self.repr {
            Repr::Tcp(t) => t.poll_write_ready(cx),
            #[cfg(unix)]
            Repr::Unix(u) => u.poll_write_ready(cx),
        }
    }

    #[inline]
    fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_write(buf),
            #[cfg(unix)]
            Repr::Unix(u) => u.try_write(buf),
        }
    }

    #[inline]
    fn try_write_vectored(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match &self.repr {
            Repr::Tcp(t) => t.try_write_vectored(bufs),
            #[cfg(unix)]
            Repr::Unix(u) => u.try_write_vectored(bufs),
        }
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        match &self.repr {
            Repr::Tcp(t) => AsyncWrite::is_write_vectored(t),
            #[cfg(unix)]
            Repr::Unix(u) => AsyncWrite::is_write_vectored(u),
        }
    }
}

// ===== Tokio::io =====

impl AsyncRead for IoStream {
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.repr {
            Repr::Tcp(t) => Pin::new(t).poll_read(cx, buf),
            #[cfg(unix)]
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
            #[cfg(unix)]
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
            #[cfg(unix)]
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
            #[cfg(unix)]
            Repr::Unix(u) => Pin::new(u).poll_shutdown(cx),
        }
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        match &self.repr {
            Repr::Tcp(t) => AsyncWrite::is_write_vectored(t),
            #[cfg(unix)]
            Repr::Unix(u) => AsyncWrite::is_write_vectored(u),
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
            #[cfg(unix)]
            Repr::Unix(u) => Pin::new(u).poll_write_vectored(cx, bufs),
        }
    }
}

