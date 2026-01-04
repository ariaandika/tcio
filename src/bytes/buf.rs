use std::io::IoSlice;

use crate::bytes::take::Take;

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

    /// Consumes `len` bytes inside self and returns new instance of [`Bytes`][super::Bytes] with
    /// this data.
    ///
    /// This function may be optimized by the underlying type to avoid actual copies. For example,
    /// [`Bytes`][super::Bytes] implementation will do a shallow copy (ref-count increment).
    ///
    /// # Panics
    ///
    /// This function panics if `len > self.remaining()`.
    fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
        use crate::bytes::BufMut;
        assert!(len <= self.remaining(), "``len is larger than the remaining buf");
        let mut ret = crate::bytes::BytesMut::with_capacity(len);
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
}

/// This macro make sure to forward ALL methods which may be overriden by the implementor.
///
/// Otherwise, it will use default implementation.
macro_rules! delegate {
    () => {
        #[inline]
        fn remaining(&self) -> usize {
            T::remaining(self)
        }

        #[inline]
        fn chunk(&self) -> &[u8] {
            T::chunk(self)
        }

        #[inline]
        fn advance(&mut self, cnt: usize) {
            T::advance(self, cnt);
        }

        #[inline]
        fn chunks_vectored<'a>(&'a self, dst: &mut [std::io::IoSlice<'a>]) -> usize {
            T::chunks_vectored(self, dst)
        }

        #[inline]
        fn has_remaining(&self) -> bool {
            T::has_remaining(self)
        }

        #[inline]
        fn copy_to_slice(&mut self, dst: &mut [u8]) {
            T::copy_to_slice(self, dst)
        }

        #[inline]
        fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
            T::copy_to_bytes(self, len)
        }
    };
}

impl<T: Buf + ?Sized> Buf for &mut T {
    delegate!();
}

impl<T: Buf + ?Sized> Buf for Box<T> {
    delegate!();
}

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
}

// assert Buf is dyn compatible.
fn _assert_trait_object(_b: &dyn Buf) {}
