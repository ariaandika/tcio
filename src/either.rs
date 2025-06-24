use std::{fmt, io, pin::Pin};

use crate::EitherMap;

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

// ===== Projection =====

enum EitherProject<'p, L, R>
where
    Either<L, R>: 'p,
{
    Left(Pin<&'p mut L>),
    Right(Pin<&'p mut R>),
}

impl<L, R> Either<L, R> {
    #[inline]
    fn project<'p>(self: Pin<&'p mut Self>) -> EitherProject<'p, L, R> {
        // SAFETY: self is pinned
        // no `Drop`, nor manual `Unpin` implementation.
        unsafe {
            match self.get_unchecked_mut() {
                Self::Left(l) => EitherProject::Left(Pin::new_unchecked(l)),
                Self::Right(r) => EitherProject::Right(Pin::new_unchecked(r)),
            }
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
        match self.project() {
            EitherProject::Left(pin) => pin.poll(cx),
            EitherProject::Right(pin) => pin.poll(cx),
        }
    }
}

// ===== Either traits =====

impl<L: fmt::Display, R: fmt::Display> fmt::Display for Either<L, R> {
    #[inline]
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

