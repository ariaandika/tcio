use std::cmp;
use std::io::IoSlice;
use std::mem;

use crate::bytes::Buf;

/// A [`Buf`] adapter which limits the bytes read from an underlying buffer.
///
/// This struct is generally created by calling `take()` on `Buf`. See documentation of
/// [`take()`][Buf::take] for more details.
#[derive(Debug)]
pub struct Take<T> {
    inner: T,
    limit: usize,
}

impl<T> Take<T> {
    pub(crate) fn new(inner: T, limit: usize) -> Self {
        Self { inner, limit }
    }

    /// Consumes this `Take`, returns the underlying value.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Buf> Buf for Take<T> {
    fn remaining(&self) -> usize {
        cmp::min(self.inner.remaining(), self.limit)
    }

    fn chunk(&self) -> &[u8] {
        let bytes = self.inner.chunk();
        &bytes[..cmp::min(bytes.len(), self.limit)]
    }

    fn advance(&mut self, cnt: usize) {
        assert!(cnt <= self.limit, "advancing out of limit bound");
        self.inner.advance(cnt);
        self.limit -= cnt;
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        if self.limit == 0 {
            return 0;
        }

        const LEN: usize = 32;
        let mut slices: [IoSlice<'a>; LEN] = [IoSlice::new(&[]); LEN];

        let cnt = self
            .inner
            .chunks_vectored(&mut slices[..dst.len().min(LEN)]);
        let mut limit = self.limit;

        for (i, (dst, slice)) in dst[..cnt].iter_mut().zip(slices.into_iter()).enumerate() {
            if let Some(buf) = slice.get(..limit) {
                // cannot use the unstable `IoSlice::as_slice`
                let buf = unsafe { mem::transmute::<&[u8], &'a [u8]>(buf) };
                *dst = IoSlice::new(buf);
                return i + 1;
            } else {
                *dst = slice;
                limit -= slice.len();
            }
        }

        cnt
    }

    fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
        assert!(len <= self.limit, "`len` is out of limit bound");
        let bytes = self.inner.copy_to_bytes(len);
        self.limit -= len;
        bytes
    }
}
