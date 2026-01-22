use crate::bytes::{Buf, BufMut, UninitSlice};
use std::{
    io::{self, IoSlice},
    pin::Pin,
    task::{Poll, ready},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Try to read from [`AsyncRead`] and write to [`BufMut`].
pub fn poll_read<B, IO>(
    buf: &mut B,
    io: &mut IO,
    cx: &mut std::task::Context,
) -> Poll<io::Result<usize>>
where
    B: BufMut + ?Sized,
    IO: AsyncRead + Unpin,
{
    if !buf.has_remaining_mut() {
        return Poll::Ready(Ok(0));
    }

    let n = {
        let dst = buf.chunk_mut();
        let mut buf = ReadBuf::from(dst);
        let ptr = buf.filled().as_ptr();
        ready!(Pin::new(io).poll_read(cx, &mut buf)?);

        // Ensure the pointer does not change from under us
        assert_eq!(ptr, buf.filled().as_ptr());
        buf.filled().len()
    };

    // Safety: This is guaranteed to be the number of initialized (and read)
    // bytes due to the invariants provided by `ReadBuf::filled`.
    unsafe {
        buf.advance_mut(n);
    }

    Poll::Ready(Ok(n))
}

/// Try to read from [`Buf`] and write to [`AsyncRead`].
pub fn poll_write_all<B, IO>(
    buf: &mut B,
    io: &mut IO,
    cx: &mut std::task::Context,
) -> Poll<io::Result<()>>
where
    B: Buf + ?Sized,
    IO: AsyncWrite + Unpin,
{
    const MAX_VECTOR_ELEMENTS: usize = 64;

    while buf.has_remaining() {
        let n = if io.is_write_vectored() {
            let mut slices = [IoSlice::new(&[]); MAX_VECTOR_ELEMENTS];
            let cnt = buf.chunks_vectored(&mut slices);
            ready!(Pin::new(&mut *io).poll_write_vectored(cx, &slices[..cnt]))?
        } else {
            ready!(Pin::new(&mut *io).poll_write(cx, buf.chunk())?)
        };
        buf.advance(n);
        if n == 0 {
            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
        }
    }

    Poll::Ready(Ok(()))
}

impl<'a> From<&'a mut UninitSlice> for ReadBuf<'a> {
    #[inline]
    fn from(value: &'a mut UninitSlice) -> Self {
        ReadBuf::uninit(unsafe { value.as_uninit_slice_mut() })
    }
}

