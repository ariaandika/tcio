use core::mem::{self, MaybeUninit};
use core::ptr;

/// Represent a writable in memory buffer.
pub trait BufMut {
    /// Returns the remaining capacity left this buffer can hold.
    fn remaining_mut(&self) -> usize;

    /// Returns the unitialized bytes this buffer holds.
    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>];

    /// Advance the amount of initialized bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the next `cnt` bytes of `chunk` are
    /// initialized.
    unsafe fn advance_mut(&mut self, cnt: usize);

    /// Returns `true` if there is more capacity left remaining.
    #[inline]
    fn has_remaining_mut(&self) -> bool {
        self.remaining_mut() > 0
    }

    /// Put a slice into buffer.
    #[inline]
    fn put_slice(&mut self, mut src: &[u8]) {
        assert!(src.len() <= self.remaining_mut());

        while !src.is_empty() {
            let dst = self.chunk_mut();
            let cnt = usize::min(src.len(), dst.len());

            BufMut::put_slice(&mut &mut dst[..cnt], &src[..cnt]);
            src = &src[cnt..];

            // SAFETY: We just initialized `cnt` bytes in `self`.
            unsafe { self.advance_mut(cnt) };
        }
    }
}

impl BufMut for &mut [u8] {
    #[inline]
    fn remaining_mut(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe { mem::transmute(&mut **self) }
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        // taken from `impl Write for &mut [u8]`.
        let (_, b) = unsafe { mem::take(self).split_at_mut_unchecked(cnt) };
        *self = b;
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        // taken from `impl Write for &mut [u8]`.
        let (a, b) = mem::take(self).split_at_mut(src.len());
        a.copy_from_slice(src);
        *self = b;
    }
}

impl BufMut for &mut [MaybeUninit<u8>] {
    #[inline]
    fn remaining_mut(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        // taken from `impl Write for &mut [u8]`.
        let (_, b) = unsafe { mem::take(self).split_at_mut_unchecked(cnt) };
        *self = b;
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        let (a, b) = mem::take(self).split_at_mut(src.len());
        unsafe { ptr::copy_nonoverlapping(src.as_ptr(), a.as_mut_ptr().cast(), src.len()); };
        *self = b;
    }
}

impl BufMut for Vec<u8> {
    #[inline]
    fn remaining_mut(&self) -> usize {
        isize::MAX as usize - self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        if self.capacity() == self.len() {
            self.reserve(64);
        }

        self.spare_capacity_mut()
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        unsafe { self.set_len(self.len() + cnt); }
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        self.extend_from_slice(src);
    }
}
