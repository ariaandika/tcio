use std::io::IoSlice;

use crate::bytes::{Bytes, BytesMut, Chain, Take};

macro_rules! fn_get_int {
    ($f:ident, $doc:literal, $($ty:ty, $m1:ident, $m2:ident),* $(,)?) => {
        $(
            #[doc = concat!("Get `", stringify!($ty), "` in ", $doc)]
            ///
            /// # Panics
            ///
            /// Panics if there is not enough remaining bytes.
            #[inline]
            fn $m1(&mut self) -> $ty {
                match self.$m2() {
                    Some(ok) => ok,
                    None => remaining_fail(self.remaining(), size_of::<$ty>())
                }
            }

            #[doc = concat!("Get `", stringify!($ty), "` in ", $doc)]
            ///
            /// Returns `None` if there is not enough remaining bytes.
            fn $m2(&mut self) -> Option<$ty> {
                const SIZE: usize = size_of::<$ty>();

                if self.remaining() < SIZE {
                    return None;
                }

                let bytes = if let Ok(&ok) = self.chunk().try_into() {
                    // buf is contiguous
                    self.advance(SIZE);
                    ok
                } else {
                    // buf is not contiguous
                    let old_rem = self.remaining();
                    let mut bytes = [0u8; SIZE];
                    let mut tmp = &mut bytes[..];

                    while !tmp.is_empty() {
                        let chunk = self.chunk();
                        let cnt = chunk.len().min(tmp.len());
                        let (dst, rest) = tmp.split_at_mut(cnt);
                        dst.copy_from_slice(&chunk[..cnt]);
                        tmp = rest;
                        self.advance(cnt);
                    }

                    // this helps the compiler, because `self.advance()` is inside while loop, compiler
                    // cannot rely on it
                    unsafe { std::hint::assert_unchecked(self.remaining() == old_rem - SIZE) };

                    bytes
                };

                Some(<$ty>::$f(bytes))
            }
        )*
    };
}

macro_rules! impl_get_int_contiguous {
    (u8, $m:ident, $f:ident) => {
        fn $m(&mut self) -> Option<u8> {
            let &chunk = self.chunk().first()?;
            self.advance(1);
            Some(chunk)
        }
    };
    // u16, try_from_u16
    ($ty:ident, $m:ident, $f:ident) => {
        fn $m(&mut self) -> Option<$ty> {
            const SIZE: usize = size_of::<$ty>();
            let &chunk = self.chunk().first_chunk::<SIZE>()?;
            self.advance(SIZE);
            Some($ty::$f(chunk))
        }
    };
    ($f:ident, $($ty:ident, $m:ident),* $(,)?) => {
        $(
            impl_get_int_contiguous!($ty, $m, $f);
        )*
    };
    () => {
        impl_get_int_contiguous!(
            from_be_bytes,
            u8, try_get_u8,
            i8, try_get_i8,
            u16, try_get_u16,
            i16, try_get_i16,
            u32, try_get_u32,
            i32, try_get_i32,
            u64, try_get_u64,
            i64, try_get_i64,
            u128, try_get_u128,
            i128, try_get_i128,
        );
        impl_get_int_contiguous!(
            from_le_bytes,
            u8, try_get_u8_le,
            i8, try_get_i8_le,
            u16, try_get_u16_le,
            i16, try_get_i16_le,
            u32, try_get_u32_le,
            i32, try_get_i32_le,
            u64, try_get_u64_le,
            i64, try_get_i64_le,
            u128, try_get_u128_le,
            i128, try_get_i128_le,
        );
        impl_get_int_contiguous!(
            from_ne_bytes,
            u8, try_get_u8_ne,
            i8, try_get_i8_ne,
            u16, try_get_u16_ne,
            i16, try_get_i16_ne,
            u32, try_get_u32_ne,
            i32, try_get_i32_ne,
            u64, try_get_u64_ne,
            i64, try_get_i64_ne,
            u128, try_get_u128_ne,
            i128, try_get_i128_ne,
        );
    };
}

/// Read bytes from a buffer.
///
/// A buffer stores bytes in memory such that read operations are infallible. The underlying
/// storage may or may not be in contiguous memory. A `Buf` value is a cursor into the buffer.
/// Reading from `Buf` advances the cursor position. It can be thought of as an efficient
/// `Iterator` for collections of bytes.
///
/// The simplest `Buf` is a `&[u8]`.
pub trait Buf {
    /// Returns the number of bytes between the current position and the end of the buffer.
    ///
    /// This value is greater than or equal to the length of the slice returned by `chunk()`.
    ///
    /// # Implementation notes
    ///
    /// Implementations of `remaining` should ensure that the return value does not change unless a
    /// call is made to `advance` or any other function that is documented to change the `Buf`'s
    /// current position.
    fn remaining(&self) -> usize;

    /// Returns a slice starting at the current position and of length between 0 and
    /// `Buf::remaining()`.
    ///
    /// Note that this *can* return a shorter slice (this allows non-continuous internal
    /// representation).
    ///
    /// This is a lower level function. Most operations are done with other functions.
    ///
    /// # Implementation notes
    ///
    /// This function should never panic. `chunk()` should return an empty slice **if and only if**
    /// `remaining()` returns 0. In other words, `chunk()` returning an empty slice implies that
    /// `remaining()` will return 0 and `remaining()` returning 0 implies that `chunk()` will
    /// return an empty slice.
    fn chunk(&self) -> &[u8];

    /// Advance the internal cursor of the Buf.
    ///
    /// The next call to `chunk()` will return a slice starting `cnt` bytes
    /// further into the underlying buffer.
    ///
    /// # Panics
    ///
    /// This function **may** panic if `cnt > self.remaining()`.
    ///
    /// # Implementation notes
    ///
    /// It is recommended for implementations of `advance` to panic if `cnt >
    /// self.remaining()`. If the implementation does not panic, the call must
    /// behave as if `cnt == self.remaining()`.
    ///
    /// A call with `cnt == 0` should never panic and be a no-op.
    fn advance(&mut self, cnt: usize);

    /// Fills `dst` with potentially multiple slices starting at `self`'s current position.
    ///
    /// If the `Buf` is backed by disjoint slices of bytes, `chunk_vectored` enables fetching more
    /// than one slice at once. `dst` is a slice of `IoSlice` references, enabling the slice to be
    /// directly used with [`writev`] without any further conversion. The sum of the lengths of all
    /// the buffers written to `dst` will be less than or equal to `Buf::remaining()`.
    ///
    /// The entries in `dst` will be overwritten, but the data **contained** by the slices **will
    /// not** be modified. The return value is the number of slices written to `dst`. If
    /// `Buf::remaining()` is non-zero, then this writes at least one non-empty slice to `dst`.
    ///
    /// This is a lower level function. Most operations are done with other functions.
    ///
    /// # Implementation notes
    ///
    /// This function should never panic. Once the end of the buffer is reached, i.e.,
    /// `Buf::remaining` returns 0, calls to `chunk_vectored` must return 0 without mutating `dst`.
    ///
    /// Implementations should also take care to properly handle being called with `dst` being a
    /// zero length slice.
    ///
    /// [`writev`]: http://man7.org/linux/man-pages/man2/readv.2.html
    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        if dst.is_empty() {
            return 0;
        }

        if self.has_remaining() {
            dst[0] = IoSlice::new(self.chunk());
            1
        } else {
            0
        }
    }

    /// Returns `true` if there are any more bytes to consume.
    ///
    /// This is equivalent to `self.remaining() != 0`.
    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    /// Copies bytes from `self` into `dst`.
    ///
    /// The cursor is advanced by the number of bytes copied. `self` must have enough remaining
    /// bytes to fill `dst`.
    ///
    /// # Panics
    ///
    /// This function panics if `dst.len() > self.remaining()`.
    fn copy_to_slice(&mut self, mut dst: &mut [u8]) {
        assert!(dst.len() <= self.remaining(), "target slice is larger than the remaining buf");
        while !dst.is_empty() {
            let src = self.chunk();
            let cnt = usize::min(src.len(), dst.len());

            dst[..cnt].copy_from_slice(&src[..cnt]);
            dst = &mut dst[cnt..];

            self.advance(cnt);
        }
    }

    // fn try_copy_to_slice() { }

    /// Consumes `len` bytes inside self and returns new instance of [`Bytes`] with
    /// this data.
    ///
    /// This function may be optimized by the underlying type to avoid actual copies. For example,
    /// [`Bytes`] implementation will do a shallow copy (ref-count increment).
    ///
    /// # Panics
    ///
    /// This function panics if `len > self.remaining()`.
    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        use crate::bytes::BufMut;
        assert!(len <= self.remaining(), "``len is larger than the remaining buf");
        let mut ret = BytesMut::with_capacity(len);
        ret.put(self.take(len));
        ret.freeze()
    }

    /// Creates an adaptor which will read at most `limit` bytes from `self`.
    ///
    /// This function returns a new instance of `Buf` which will read at most `limit` bytes.
    fn take(self, limit: usize) -> Take<Self>
    where
        Self: Sized,
    {
        Take::new(self, limit)
    }

    /// Creates an adaptor which will chain this buffer with another.
    ///
    /// The returned `Buf` instance will first consume all bytes from `self`. Afterwards the output
    /// is equivalent to the output of next.
    fn chain<U: Buf>(self, next: U) -> Chain<Self, U>
    where
        Self: Sized,
    {
        Chain::new(self, next)
    }

    fn_get_int!(
        from_be_bytes, "big endian",
        u8, get_u8, try_get_u8,
        i8, get_i8, try_get_i8,
        u16, get_u16, try_get_u16,
        i16, get_i16, try_get_i16,
        u32, get_u32, try_get_u32,
        i32, get_i32, try_get_i32,
        u64, get_u64, try_get_u64,
        i64, get_i64, try_get_i64,
        u128, get_u128, try_get_u128,
        i128, get_i128, try_get_i128,
    );
    fn_get_int!(
        from_le_bytes, "little endian",
        u8, get_u8_le, try_get_u8_le,
        i8, get_i8_le, try_get_i8_le,
        u16, get_u16_le, try_get_u16_le,
        i16, get_i16_le, try_get_i16_le,
        u32, get_u32_le, try_get_u32_le,
        i32, get_i32_le, try_get_i32_le,
        u64, get_u64_le, try_get_u64_le,
        i64, get_i64_le, try_get_i64_le,
        u128, get_u128_le, try_get_u128_le,
        i128, get_i128_le, try_get_i128_le,
    );
    fn_get_int!(
        from_ne_bytes, "native endian",
        u8, get_u8_ne, try_get_u8_ne,
        i8, get_i8_ne, try_get_i8_ne,
        u16, get_u16_ne, try_get_u16_ne,
        i16, get_i16_ne, try_get_i16_ne,
        u32, get_u32_ne, try_get_u32_ne,
        i32, get_i32_ne, try_get_i32_ne,
        u64, get_u64_ne, try_get_u64_ne,
        i64, get_i64_ne, try_get_i64_ne,
        u128, get_u128_ne, try_get_u128_ne,
        i128, get_i128_ne, try_get_i128_ne,
    );
}

// Buf is dyn compatible.
fn _assert_trait_object(_b: &dyn Buf) {}

// ===== impl Buf =====

impl Buf for &[u8] {
    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        *self = &self[cnt..];
    }

    #[inline]
    fn copy_to_slice(&mut self, dst: &mut [u8]) {
        dst.copy_from_slice(&self[..dst.len()]);
        self.advance(dst.len());
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        let (a, b) = self.split_at(len);
        *self = b;
        Bytes::copy_from_slice(a)
    }

    impl_get_int_contiguous!();
}

impl Buf for Bytes {
    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.as_slice()
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        Self::advance(self, cnt);
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        self.split_to(len)
    }

    impl_get_int_contiguous!();
}

impl Buf for BytesMut {
    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.as_slice()
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.len(),
            "cannot advance past `len`: {:?} <= {:?}",
            cnt,
            self.len(),
        );
        unsafe {
            // SAFETY: `cnt <= self.len`, and `self.len <= self.cap`
            self.advance_unchecked(cnt);
        }
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        self.split_to(len).freeze()
    }
}

// ===== blanket impl =====

/// This macro make sure to forward methods which may be overriden by the implementor.
///
/// Otherwise, it will use default implementation.
macro_rules! delegate_blanket_impl {
    () => {
        #[inline] fn remaining(&self) -> usize { T::remaining(self) }
        #[inline] fn chunk(&self) -> &[u8] { T::chunk(self) }
        #[inline] fn advance(&mut self, cnt: usize) { T::advance(self, cnt); }
        #[inline] fn chunks_vectored<'a>(&'a self, dst: &mut [std::io::IoSlice<'a>])
            -> usize { T::chunks_vectored(self, dst) }
        #[inline] fn has_remaining(&self) -> bool { T::has_remaining(self) }
        #[inline] fn copy_to_slice(&mut self, dst: &mut [u8]) { T::copy_to_slice(self, dst) }
        #[inline] fn copy_to_bytes(&mut self, len: usize) -> super::Bytes { T::copy_to_bytes(self, len) }
    };
}

impl<T: Buf + ?Sized> Buf for &mut T {
    delegate_blanket_impl!();
}

impl<T: Buf + ?Sized> Buf for Box<T> {
    delegate_blanket_impl!();
}

// ===== panics =====

// The panic code path was put into a cold function to not bloat the call site.

#[cfg_attr(not(panic = "immediate-abort"), inline(never), cold)]
#[cfg_attr(panic = "immediate-abort", inline)]
#[track_caller]
fn remaining_fail(src_len: usize, req_len: usize) -> ! {
    panic!(
        "source remaining ({src_len}) is less than requested length ({req_len})"
    )
}
