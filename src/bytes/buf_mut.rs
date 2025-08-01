use core::{
    mem::{self, MaybeUninit},
    ptr,
};

pub trait BufMut {
    fn remaining_mut(&self) -> usize;

    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>];

    /// # Safety
    ///
    /// The caller must ensure that the next `cnt` bytes of `chunk` are
    /// initialized.
    unsafe fn advance_mut(&mut self, cnt: usize);

    #[inline]
    fn has_remaining_mut(&self) -> bool {
        self.remaining_mut() > 0
    }

    #[inline]
    fn put_slice(&mut self, mut src: &[u8]) {
        if self.remaining_mut() < src.len() {
            panic!(
                "cannot write `{}` bytes, only `{}` is remaining",
                src.len(),
                self.remaining_mut()
            )
        }

        while !src.is_empty() {
            let dst = self.chunk_mut();
            let cnt = usize::min(src.len(), dst.len());

            // dst[..cnt].copy_from_slice(&src[..cnt]); TODO:
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
        unsafe { &mut *(*self as *mut [u8] as *mut [MaybeUninit<u8>]) }
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        debug_assert!(
            self.len() > cnt,
            "cannot write `{}` bytes, only `{}` is remaining",
            cnt,
            self.remaining_mut()
        );

        // taken from `impl Write for &mut [u8]`.
        let (_, b) = mem::take(self).split_at_mut(cnt);
        *self = b;
    }

    fn put_slice(&mut self, src: &[u8]) {
        if src.len() > self.len() {
            panic!(
                "cannot write `{}` bytes, only `{}` is remaining",
                src.len(),
                self.remaining_mut()
            )
        }

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
        debug_assert!(
            self.len() > cnt,
            "cannot write `{}` bytes, only `{}` is remaining",
            cnt,
            self.remaining_mut()
        );

        // taken from `impl Write for &mut [u8]`.
        let (_, b) = mem::take(self).split_at_mut(cnt);
        *self = b;
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        let src_len = src.len();

        if src_len > self.len() {
            panic!(
                "cannot write `{}` bytes, only `{}` is remaining",
                src.len(),
                self.remaining_mut()
            )
        }

        // SAFETY: We just checked that the pointer is valid for `src.len()` bytes.
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), self.as_mut_ptr().cast(), src_len);
            let (_, b) = mem::take(self).split_at_mut(src_len);
            *self = b;
        }
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
