use std::{fmt, io, pin::Pin};

use crate::Either;

/// Represent either type that implement the same trait.
///
/// Unlike [`Either`], traits which have an output type, like [`Iterator::Item`] and
/// [`Future::Output`], will output another either type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EitherMap<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> From<Either<L, R>> for EitherMap<L, R> {
    #[inline]
    fn from(value: Either<L, R>) -> Self {
        match value {
            Either::Left(l) => EitherMap::Left(l),
            Either::Right(r) => EitherMap::Right(r),
        }
    }
}

// ===== Projection =====

enum EitherMapProject<'p, L, R>
where
    EitherMap<L, R>: 'p,
{
    Left(Pin<&'p mut L>),
    Right(Pin<&'p mut R>),
}

impl<L, R> EitherMap<L, R> {
    #[inline]
    fn project<'p>(self: Pin<&'p mut Self>) -> EitherMapProject<'p, L, R> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => EitherMapProject::Left(Pin::new_unchecked(l)),
                Self::Right(r) => EitherMapProject::Right(Pin::new_unchecked(r)),
            }
        }
    }
}

impl<L: Future, R: Future> Future for EitherMap<L, R> {
    type Output = EitherMap<L::Output, R::Output>;

    #[inline]
    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.project() {
            EitherMapProject::Left(pin) => pin.poll(cx).map(EitherMap::Left),
            EitherMapProject::Right(pin) => pin.poll(cx).map(EitherMap::Right),
        }
    }
}

// ===== Either traits =====

// Cannot implement deref because it is enforced to return `&Target`, thus creating another
// reference either type inside function is not possible.

impl<L: fmt::Display, R: fmt::Display> fmt::Display for EitherMap<L, R> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Left(l) => l.fmt(f),
            Self::Right(r) => r.fmt(f),
        }
    }
}

impl<L: Iterator, R: Iterator> Iterator for EitherMap<L, R> {
    type Item = EitherMap<L::Item, R::Item>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Left(l) => l.next().map(EitherMap::Left),
            Self::Right(r) => r.next().map(EitherMap::Right),
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

impl<L: std::error::Error, R: std::error::Error> std::error::Error for EitherMap<L, R> {
    #[inline]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Left(l) => l.source(),
            Self::Right(r) => r.source(),
        }
    }
}

impl<L: io::Read, R: io::Read> io::Read for EitherMap<L, R> {
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

impl<L: io::Write, R: io::Write> io::Write for EitherMap<L, R> {
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

impl<L: AsRef<[u8]>, R: AsRef<[u8]>> AsRef<[u8]> for EitherMap<L, R> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Left(l) => l.as_ref(),
            Self::Right(r) => r.as_ref(),
        }
    }
}

impl<L: AsRef<str>, R: AsRef<str>> AsRef<str> for EitherMap<L, R> {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            Self::Left(l) => l.as_ref(),
            Self::Right(r) => r.as_ref(),
        }
    }
}

