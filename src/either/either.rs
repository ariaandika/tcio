use std::fmt;
use std::io;
use std::pin::Pin;

use crate::either::EitherMap;

/// Represent either type that implement the same trait.
///
/// Traits which have an output type, like [`Iterator::Item`] and [`Future::Output`] requires that
/// both type have the same ouput type.
///
/// For implementation that output another either type, use [`EitherMap`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Either<L, R> {
    /// Left variant.
    Left(L),
    /// Right variant.
    Right(R),
}

impl<L, R> From<EitherMap<L, R>> for Either<L, R> {
    #[inline]
    fn from(value: EitherMap<L, R>) -> Self {
        match value {
            EitherMap::Left(l) => Self::Left(l),
            EitherMap::Right(r) => Self::Right(r),
        }
    }
}

impl<L: Future, R: Future<Output = L::Output>> Future for Either<L, R> {
    type Output = L::Output;

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll(cx),
                Self::Right(r) => Pin::new_unchecked(r).poll(cx),
            }
        }
    }
}

// ===== Either traits =====

impl<L: fmt::Display, R: fmt::Display> fmt::Display for Either<L, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left(l) => l.fmt(f),
            Self::Right(r) => r.fmt(f),
        }
    }
}

impl<L: std::ops::Deref, R: std::ops::Deref<Target = L::Target>> std::ops::Deref for Either<L, R> {
    type Target = L::Target;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Left(l) => l.deref(),
            Self::Right(r) => r.deref(),
        }
    }
}

impl<L: std::ops::DerefMut, R: std::ops::DerefMut<Target = L::Target>> std::ops::DerefMut
    for Either<L, R>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Either::Left(l) => l.deref_mut(),
            Either::Right(r) => r.deref_mut(),
        }
    }
}

impl<L: Iterator, R: Iterator<Item = L::Item>> Iterator for Either<L, R> {
    type Item = L::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Left(l) => l.next(),
            Self::Right(r) => r.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Left(l) => l.size_hint(),
            Self::Right(r) => r.size_hint(),
        }
    }
}

impl<L: std::error::Error, R: std::error::Error> std::error::Error for Either<L, R> {
    #[inline]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Left(l) => l.source(),
            Self::Right(r) => r.source(),
        }
    }
}

impl<L: io::Read, R: io::Read> io::Read for Either<L, R> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Left(l) => l.read(buf),
            Self::Right(r) => r.read(buf),
        }
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [io::IoSliceMut<'_>]) -> io::Result<usize> {
        match self {
            Self::Left(l) => l.read_vectored(bufs),
            Self::Right(r) => r.read_vectored(bufs),
        }
    }
}

impl<L: io::Write, R: io::Write> io::Write for Either<L, R> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Left(l) => l.write(buf),
            Self::Right(r) => r.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Left(l) => l.flush(),
            Self::Right(r) => r.flush(),
        }
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match self {
            Self::Left(l) => l.write_vectored(bufs),
            Self::Right(r) => r.write_vectored(bufs),
        }
    }
}

impl<L: AsRef<[u8]>, R: AsRef<[u8]>> AsRef<[u8]> for Either<L, R> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Left(l) => l.as_ref(),
            Self::Right(r) => r.as_ref(),
        }
    }
}

impl<L: AsRef<str>, R: AsRef<str>> AsRef<str> for Either<L, R> {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            Self::Left(l) => l.as_ref(),
            Self::Right(r) => r.as_ref(),
        }
    }
}

#[cfg(feature = "tokio")]
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "tokio")]
impl<L: AsyncRead, R: AsyncRead> AsyncRead for Either<L, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll_read(cx, buf),
                Self::Right(r) => Pin::new_unchecked(r).poll_read(cx, buf),
            }
        }
    }
}

#[cfg(feature = "tokio")]
impl<L: AsyncWrite, R: AsyncWrite> AsyncWrite for Either<L, R> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll_write(cx, buf),
                Self::Right(r) => Pin::new_unchecked(r).poll_write(cx, buf),
            }
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll_flush(cx),
                Self::Right(r) => Pin::new_unchecked(r).poll_flush(cx),
            }
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), io::Error>> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll_shutdown(cx),
                Self::Right(r) => Pin::new_unchecked(r).poll_shutdown(cx),
            }
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, io::Error>> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => Pin::new_unchecked(l).poll_write_vectored(cx, bufs),
                Self::Right(r) => Pin::new_unchecked(r).poll_write_vectored(cx, bufs),
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Left(l) => l.is_write_vectored(),
            Self::Right(r) => r.is_write_vectored(),
        }
    }
}
