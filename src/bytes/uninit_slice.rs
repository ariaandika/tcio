use std::mem::{self, MaybeUninit};
use std::ptr;
use std::slice;

/// Uninitialized byte slice.
///
/// Returned by `BufMut::chunk_mut()`, the referenced byte slice may be uninitialized. The wrapper
/// provides safe access without introducing undefined behavior.
///
/// The safety invariants of this wrapper are:
///
///  1. Reading from an `UninitSlice` is undefined behavior.
///  2. Writing uninitialized bytes to an `UninitSlice` is undefined behavior.
///
/// The difference between `&mut UninitSlice` and `&mut [MaybeUninit<u8>]` is that it is possible
/// in safe code to write uninitialized bytes to an `&mut [MaybeUninit<u8>]`, which this type
/// prohibits.
#[repr(transparent)]
pub struct UninitSlice([MaybeUninit<u8>]);

impl UninitSlice {
    /// Creates new `&mut UninitSlice` from initialized bytes.
    #[inline]
    pub const fn new(bytes: &mut [u8]) -> &mut Self {
        unsafe { &mut *(bytes as *mut [u8] as *mut Self) }
    }

    /// Creates new `&mut UninitSlice` from uninitialized bytes.
    #[inline]
    pub const fn from_uninit(bytes: &mut [MaybeUninit<u8>]) -> &mut Self {
        unsafe { &mut *(bytes as *mut [MaybeUninit<u8>] as *mut Self) }
    }

    fn from_uninit_ref(bytes: &[MaybeUninit<u8>]) -> &Self {
        unsafe { &*(bytes as *const [MaybeUninit<u8>] as *const Self) }
    }

    /// Create a `&mut UninitSlice` from a pointer and a length.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `ptr` references a valid memory region owned by the caller
    /// representing a byte slice for the duration of `'a`.
    #[inline]
    pub const unsafe fn from_raw_parts_mut<'a>(ptr: *mut u8, len: usize) -> &'a mut UninitSlice {
        unsafe { Self::from_uninit(slice::from_raw_parts_mut(ptr as *mut _, len)) }
    }

    /// Returns the number of bytes in the slice.
    #[inline]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the slice has a length of 0.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an unsafe mutable pointer to the bytes.
    ///
    /// # Safety
    ///
    /// The caller **must not** read from the referenced memory and **must not** write
    /// **uninitialized** bytes to the slice either.
    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr().cast()
    }

    /// Copies all bytes from `src` into `self`.
    ///
    /// The length of `src` must be the same as `self`.
    ///
    /// # Panics
    ///
    /// This function will panic if the two slices have different lengths.
    #[inline]
    pub fn copy_from_slice(&mut self, src: &[u8]) {
        assert_eq!(self.len(), src.len());
        unsafe { ptr::copy_nonoverlapping(src.as_ptr(), self.as_mut_ptr(), self.len()) };
    }

    /// Return a `&mut [MaybeUninit<u8>]` to this slice's buffer.
    ///
    /// # Safety
    ///
    /// The caller **must not** read from the referenced memory and **must not** write
    /// **uninitialized** bytes to the slice either. This is because `BufMut` implementation that
    /// created the `UninitSlice` knows which parts are initialized. Writing uninitialized bytes to
    /// the slice may cause the `BufMut` to read those bytes and trigger undefined behavior.
    #[inline]
    pub unsafe fn as_uninit_slice_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.0
    }
}

impl std::fmt::Debug for UninitSlice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("UninitBytes").field(&..self.0.len()).finish()
    }
}

impl<'a> From<&'a mut [u8]> for &'a mut UninitSlice {
    #[inline]
    fn from(slice: &'a mut [u8]) -> Self {
        UninitSlice::new(slice)
    }
}

impl<'a> From<&'a mut [MaybeUninit<u8>]> for &'a mut UninitSlice {
    #[inline]
    fn from(slice: &'a mut [MaybeUninit<u8>]) -> Self {
        UninitSlice::from_uninit(slice)
    }
}

impl crate::bytes::BufMut for &mut UninitSlice {
    #[inline]
    fn remaining_mut(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut UninitSlice {
        self
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        // taken from `impl Write for &mut [u8]`.
        let (_, b) = unsafe {
            mem::replace(self, UninitSlice::from_uninit(&mut []))
                .as_uninit_slice_mut()
                .split_at_mut_unchecked(cnt)
        };
        *self = UninitSlice::from_uninit(b);
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        unsafe {
            let (a, b) = mem::replace(self, UninitSlice::from_uninit(&mut []))
                .as_uninit_slice_mut()
                .split_at_mut(src.len());
            ptr::copy_nonoverlapping(src.as_ptr(), a.as_mut_ptr().cast(), src.len());
            *self = UninitSlice::from_uninit(b);
        }
    }
}

macro_rules! impl_index {
    ($($t:ty),*) => {
        $(
            impl std::ops::Index<$t> for UninitSlice {
                type Output = UninitSlice;

                #[inline]
                fn index(&self, index: $t) -> &UninitSlice {
                    UninitSlice::from_uninit_ref(&self.0[index])
                }
            }

            impl std::ops::IndexMut<$t> for UninitSlice {
                #[inline]
                fn index_mut(&mut self, index: $t) -> &mut UninitSlice {
                    UninitSlice::from_uninit(&mut self.0[index])
                }
            }
        )*
    };
}

impl_index!(
    std::ops::Range<usize>,
    std::ops::RangeFrom<usize>,
    std::ops::RangeFull,
    std::ops::RangeInclusive<usize>,
    std::ops::RangeTo<usize>,
    std::ops::RangeToInclusive<usize>
);

