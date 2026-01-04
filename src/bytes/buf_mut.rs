use core::mem::{self, MaybeUninit};
use core::ptr;

use crate::bytes::Chain;

/// A trait for values that provide sequential write access to bytes.
///
/// Write bytes to a buffer
///
/// A buffer stores bytes in memory such that write operations are infallible. The underlying
/// storage may or may not be in contiguous memory. A `BufMut` value is a cursor into the buffer.
/// Writing to `BufMut` advances the cursor position.
///
/// The simplest `BufMut` is a `Vec<u8>`.
pub trait BufMut {
    /// Returns the number of bytes that can be written from the current position until the end of
    /// the buffer is reached.
    ///
    /// This value is greater than or equal to the length of the slice returned by `chunk_mut()`.
    ///
    /// Writing to a `BufMut` may involve allocating more memory on the fly. Implementations may
    /// fail before reaching the number of bytes indicated by this method if they encounter an
    /// allocation failure.
    ///
    /// # Implementation notes
    ///
    /// Implementations of `remaining_mut` should ensure that the return value does not change
    /// unless a call is made to `advance_mut` or any other function that is documented to change
    /// the `BufMut`'s current position.
    ///
    /// # Note
    ///
    /// `remaining_mut` may return value smaller than actual available space.
    fn remaining_mut(&self) -> usize;

    /// Returns a mutable slice starting at the current BufMut position and of length between 0 and
    /// `BufMut::remaining_mut()`. Note that this *can* be shorter than the whole remainder of the
    /// buffer (this allows non-continuous implementation).
    ///
    /// This is a lower level function. Most operations are done with other functions.
    ///
    /// # Implementation notes
    ///
    /// This function should never panic. `chunk_mut()` should return an empty slice **if and only
    /// if** `remaining_mut()` returns 0. In other words, `chunk_mut()` returning an empty slice
    /// implies that `remaining_mut()` will return 0 and `remaining_mut()` returning 0 implies that
    /// `chunk_mut()` will return an empty slice.
    ///
    /// This function may trigger an out-of-memory abort if it tries to allocate memory and fails
    /// to do so.
    fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>];

    /// Advance the internal cursor of the BufMut
    ///
    /// The next call to `chunk_mut` will return a slice starting `cnt` bytes further into the
    /// underlying buffer.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the next `cnt` bytes of `chunk` are initialized.
    ///
    /// The caller also must ensure that `cnt <= self.remaining_mut()`.
    ///
    /// # Implementation notes
    ///
    /// A call with `cnt == 0` should never panic and be a no-op.
    unsafe fn advance_mut(&mut self, cnt: usize);

    /// Returns true if there is space in `self` for more bytes.
    ///
    /// This is equivalent to `self.remaining_mut() != 0`.
    #[inline]
    fn has_remaining_mut(&self) -> bool {
        self.remaining_mut() > 0
    }

    /// Transfer bytes into `self` from `src` and advance the cursor by the number of bytes
    /// written.
    ///
    /// # Panics
    ///
    /// Panics if `self` does not have enough capacity to contain `src`.
    fn put<T: super::Buf>(&mut self, mut src: T)
    where
        // this is required for BufMut to be dyn compatible
        Self: Sized
    {
        assert!(src.remaining() <= self.remaining_mut());

        while src.has_remaining() {
            // SAFETY: MaybeUninit<u8>` is guaranteed to have the same size,
            // alignment as `u8`, and we did not perform any read
            let dst = unsafe {
                mem::transmute::<&mut [MaybeUninit<u8>], &mut [u8]>(self.chunk_mut())
            };
            let s = src.chunk();
            let cnt = usize::min(s.len(), dst.len());

            dst[..cnt].copy_from_slice(&s[..cnt]);

            // SAFETY: We just initialized `cnt` bytes in `self`.
            unsafe { self.advance_mut(cnt) };
            src.advance(cnt);
        }
    }

    /// Transfer bytes into `self` from `src` and advance the cursor by the
    /// number of bytes written.
    ///
    /// `self` must have enough remaining capacity to contain all of `src`.
    ///
    /// # Panics
    ///
    /// Panics if `self` does not have enough capacity to contain `src`.
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

    /// Creates an adapter which will chain this buffer with another.
    ///
    /// The returned `BufMut` instance will first write to all bytes from `self`. Afterwards, it
    /// will write to `next`.
    #[inline]
    fn chain_mut<U: BufMut>(self, next: U) -> Chain<Self, U>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }
}

/// This macro make sure to forward ALL methods which may be overriden by the implementor.
///
/// Otherwise, it will use default implementation.
macro_rules! delegate {
    () => {
        #[inline]
        fn remaining_mut(&self) -> usize {
            T::remaining_mut(self)
        }

        #[inline]
        fn chunk_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            T::chunk_mut(self)
        }

        #[inline]
        unsafe fn advance_mut(&mut self, cnt: usize) {
            unsafe { T::advance_mut(self, cnt) }
        }

        #[inline]
        fn has_remaining_mut(&self) -> bool {
            T::has_remaining_mut(self)
        }

        #[inline]
        fn put_slice(&mut self, src: &[u8]) {
            T::put_slice(self, src)
        }
    };
}

impl<T: BufMut + ?Sized> BufMut for &mut T {
    delegate!();
}

impl<T: BufMut + ?Sized> BufMut for Box<T> {
    delegate!();
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
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), a.as_mut_ptr().cast(), src.len());
        };
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
    fn put<T: crate::bytes::Buf>(&mut self, mut src: T) {
        self.reserve(src.remaining());

        while src.has_remaining() {
            let s = src.chunk();
            let l = s.len();
            self.extend_from_slice(s);
            src.advance(l);
        }
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        self.extend_from_slice(src);
    }
}

// assert BufMut is dyn compatible.
fn _assert_trait_object(_b: &dyn BufMut) {}
