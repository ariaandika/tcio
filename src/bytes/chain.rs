use crate::bytes::{Buf, BufMut};

/// A `Chain` sequences two buffers.
///
/// `Chain` is an adapter that links two underlying buffers and provides a continuous view across
/// both buffers. It is able to sequence either immutable buffers ([`Buf`] values) or mutable
/// buffers ([`BufMut`] values).
///
/// This struct is generally created by calling [`Buf::chain`]. Please see that function's
/// documentation for more detail.
///
/// [`Buf::chain`]: Buf::chain
#[derive(Debug)]
pub struct Chain<T, U> {
    a: T,
    b: U,
}

impl<T, U> Chain<T, U> {
    pub(crate) fn new(a: T, b: U) -> Self {
        Self { a, b }
    }

    /// Consumes this `Chain`, returns the underlying value.
    pub fn into_inner(self) -> (T, U) {
        (self.a, self.b)
    }
}

impl<T: Buf, U: Buf> Buf for Chain<T, U> {
    fn remaining(&self) -> usize {
        self.a.remaining().saturating_add(self.b.remaining())
    }

    fn chunk(&self) -> &[u8] {
        if self.a.has_remaining() {
            self.a.chunk()
        } else {
            self.b.chunk()
        }
    }

    fn advance(&mut self, cnt: usize) {
        let a_rem = self.a.remaining();
        self.a.advance(a_rem.min(cnt));
        if let Some(remain_cnt) = cnt.checked_sub(a_rem) {
            self.b.advance(remain_cnt);
        }
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [std::io::IoSlice<'a>]) -> usize {
        let mut cnt = self.a.chunks_vectored(dst);
        cnt += self.b.chunks_vectored(&mut dst[cnt..]);
        cnt
    }

    fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
        let a_rem = self.a.remaining();
        if a_rem >= len {
            self.a.copy_to_bytes(len)
        } else if a_rem == 0 {
            self.b.copy_to_bytes(len)
        } else {
            assert!(
                len <= a_rem + self.b.remaining(),
                "`len` out of remaining bound"
            );
            let mut bufm = crate::bytes::BytesMut::with_capacity(len);
            bufm.put(&mut self.a);
            bufm.put((&mut self.b).take(len - a_rem));
            bufm.freeze()
        }
    }
}

impl<T: BufMut, U: BufMut> BufMut for Chain<T, U> {
    fn remaining_mut(&self) -> usize {
        self.a.remaining_mut().saturating_add(self.b.remaining_mut())
    }

    fn chunk_mut(&mut self) -> &mut [std::mem::MaybeUninit<u8>] {
        if self.a.has_remaining_mut() {
            self.a.chunk_mut()
        } else {
            self.b.chunk_mut()
        }
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        let a_rem = self.a.remaining_mut();
        unsafe { self.a.advance_mut(a_rem.min(cnt)) };
        if let Some(remain_cnt) = cnt.checked_sub(a_rem) {
            unsafe { self.b.advance_mut(remain_cnt) };
        }
    }
}
