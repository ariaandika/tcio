use std::{
    mem::{self, ManuallyDrop},
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicPtr, Ordering},
};

use super::{
    BytesMut, Cursor, CursorBuf,
    shared::{self, Shared},
};

/// A cheaply cloneable and sliceable chunk of contiguous memory.
pub struct Bytes {
    ptr: NonNull<u8>,
    len: usize,
    /// it is requires to be atomic,
    /// buffer promotion requires to update the ptr
    ///
    /// 1. 0x__1, (data as usize >> 1), offset from starting ptr
    /// 2. null, static value
    /// 3. 0x_00, *mut Shared
    data: AtomicPtr<Shared>,
}

unsafe impl Send for Bytes {}
unsafe impl Sync for Bytes {}

// ===== Constructor =====

impl Bytes {
    /// Create new empty [`Bytes`].
    #[inline]
    pub const fn new() -> Self {
        Self::from_static(&[])
    }

    /// Create new [`Bytes`] from static slice.
    ///
    /// Additionally, [`is_unique`][Bytes::is_unique] will always returns `false`.
    #[inline]
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(bytes.as_ptr().cast_mut()) },
            len: bytes.len(),
            data: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Create new [`Bytes`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(data: &[u8]) -> Self {
        Self::from_vec(data.to_vec())
    }

    pub(crate) fn from_vec(mut vec: Vec<u8>) -> Self {
        if vec.is_empty() {
            return Self::new();
        }

        let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };
        let len = vec.len();
        let cap = vec.capacity();

        // `into_boxed_slice`, which call `shrink_to_fit` will only reallocate
        // if `capacity > len`
        if cap == len {
            let _vec = ManuallyDrop::new(vec);
            let data = AtomicPtr::new(shared::new_unpromoted());
            Self { ptr, len, data }
        } else {
            // PERF: we cannot start in unpromoted for `Shared` storage
            // Problems:
            // - we have nowhere to store capacity of the vector
            // - the `data` field already contains the offset from the start ptr
            // - if `len < cap`, there is a "tail offset", thus
            //   `len` cannot be treated as capacity
            // Consideration:
            // - `into_boxed_slice`: reallocate and copy the bytes, as expensive as vector length
            // - `shared::promote_with_vec`: allocate `AtomicUsize`, pointer, and capacity (3 word)

            let data = AtomicPtr::new(shared::promote_with_vec(vec, 1));
            Self { ptr, len, data }
        }
    }

    fn from_box(boxed: Box<[u8]>) -> Self {
        Self {
            len: boxed.len(),
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(boxed).cast()) },
            data: AtomicPtr::new(shared::new_unpromoted()),
        }
    }

    pub(crate) fn from_mut(shared: *mut Shared, bytesm: BytesMut) -> Self {
        debug_assert!(shared::is_promoted(shared));
        let mut bytesm = ManuallyDrop::new(bytesm);
        Self {
            ptr: unsafe { NonNull::new_unchecked(bytesm.as_mut_ptr()) },
            len: bytesm.len(),
            data: AtomicPtr::new(shared),
        }
    }

    /// Create new [`Cursor`] from current `Bytes`.
    #[inline]
    pub const fn cursor(&self) -> Cursor<'_> {
        Cursor::new(self.as_slice())
    }

    /// Create new mutable [`CursorBuf`] from current buffer.
    #[inline]
    pub const fn cursor_mut(&mut self) -> CursorBuf<&mut Self> {
        CursorBuf::<&mut Self>::shared_mut(self)
    }
}

// ===== Getters =====

impl Bytes {
    /// Returns a raw pointer to the buffer, or a dangling raw pointer valid for zero sized reads
    /// if the buffer didn't allocate.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Returns the number of bytes in the `Bytes`.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if `Bytes` contains no bytes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Extracts a slice containing the entire bytes.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    // private

    /// Specialized empty `Bytes` with given pointer.
    ///
    /// This is used when split and resulting in empty `Bytes` that does not need to inc rement the
    /// atomic counter.
    fn new_empty_with_ptr(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            len: 0,
            data: AtomicPtr::new(ptr::null_mut()),
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn data(&self) -> &AtomicPtr<Shared> {
        &self.data
    }
}

// ===== View =====

impl Bytes {
    /// Returns the shared subset of `Bytes` with given range.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let slice = bytes.slice(6..);
    /// assert_eq!(&slice, &b"World!"[..]);
    /// ```
    ///
    /// # Panics
    ///
    /// `range` should be in bounds of bytes capacity, otherwise panic.
    #[inline]
    pub fn slice(&self, range: impl core::ops::RangeBounds<usize>) -> Self {
        self.slice_bound(range.start_bound(), range.end_bound())
    }

    fn slice_bound(
        &self,
        start_bound: core::ops::Bound<&usize>,
        end_bound: core::ops::Bound<&usize>,
    ) -> Self {
        use core::ops::Bound;

        let self_len = self.len;

        let begin = match start_bound {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of range"),
            Bound::Unbounded => 0,
        };

        let end = match end_bound {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self_len,
        };

        assert!(end <= self_len, "out of bounds");

        // ASSERT:
        // 1. is `begin <= end`
        // 2. is `len <= end`
        // 3. is `end <= self.len`,
        // 4. #1, then `begin <= self.len`
        // 5. #1 and #2, then `len <= self.len`
        let len = end
            .checked_sub(begin)
            .expect("range should not be reversed");

        // SAFETY:
        // with invariant that `self.ptr` valid until `self.len` forward
        //
        // 1. is `begin` and `end` is relative to `self.ptr`, then `self.ptr <= ptr`
        // 2. is `begin <= self.len`, then `self.ptr.add(begin) <= self.len`
        // 3. - is `end <= self.len`, then `self.ptr.add(end) <= self.ptr.add(self.len)`
        //    - is `len <= end`, then `len < self.len`
        //    - then `len` correctly represent offset of
        //      `self.ptr.add(begin)` to `self.ptr.add(begin)`
        //
        // then `self.ptr.add(begin)` valid until `len` forward
        let ptr = unsafe { self.ptr.add(begin) };

        if len == 0 {
            return Bytes::new_empty_with_ptr(ptr);
        }

        let mut cloned = self.clone_inner();
        cloned.ptr = ptr;
        cloned.len = len;
        cloned
    }

    /// Returns the shared subset of `Bytes` with given slice.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let slice = bytes.slice_ref(&bytes[6..]);
    /// assert_eq!(&slice, &b"World!"[..]);
    /// ```
    ///
    /// # Panics
    ///
    /// `subset` should be contained in `Bytes` content, otherwise panic.
    pub fn slice_ref(&self, subset: &[u8]) -> Self {
        self.slice_from_raw(subset.as_ptr(), subset.len())
    }

    /// Returns the shared subset of `Bytes` with given slice raw parts.
    ///
    /// # Panics
    ///
    /// The slice from `data` up to `len` should be contained in `Bytes` content, otherwise panic.
    pub fn slice_from_raw(&self, data: *const u8, len: usize) -> Self {
        let self_addr = self.ptr.addr().get();
        let addr = data.addr();

        // this checks that input end pointer is still within buffer range
        assert!(
            addr.checked_add(len).unwrap() <= self_addr + self.len,
            "length out of bounds"
        );

        let offset = addr.checked_sub(self_addr).expect("pointer out of bounds");

        // SAFETY: this is the same as input `data` just using
        // usize offset to detach pointer provenance
        let data = unsafe { self.ptr.add(offset) };

        if len == 0 {
            return Self::new_empty_with_ptr(data);
        }

        // with assert and checked sub,
        // `data` is valid until `len` byte forward

        let mut cloned = self.clone_inner();
        cloned.ptr = data;
        cloned.len = len;
        cloned
    }

    /// Shortens the buffer, keeping the first `len` bytes and dropping the rest.
    ///
    /// If `len` is greater or equal to the `Bytes` length, this has no effect.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// bytes.truncate(5);
    /// assert_eq!(&bytes, &b"Hello"[..]);
    /// ```
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if len >= self.len {
            return;
        }
        if self.data.get_mut().is_null() {
            self.len = len;
            return;
        }
        // this introduce "tail offset",
        // which cannot be represented in unpromoted,
        // thus required to be promoted
        drop(self.split_off(len));
    }

    /// Shortens the buffer, dropping the last `off` bytes and keeping the rest.
    ///
    /// If `off` is greater to the `Bytes` length, this has no effect.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// bytes.truncate_off(7);
    /// assert_eq!(&bytes, &b"Hello"[..]);
    /// ```
    #[inline]
    pub fn truncate_off(&mut self, off: usize) {
        let Some(new_len) = self.len.checked_sub(off) else {
            return;
        };
        self.truncate(new_len);
    }

    /// Clears the buffer, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        *self = Self::new_empty_with_ptr(self.ptr);
    }

    /// Advance [`Bytes`] `cnt`-nth bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// bytes.advance(6);
    /// assert_eq!(&bytes, &b"World!"[..]);
    /// ```
    pub fn advance(&mut self, cnt: usize) {
        assert!(cnt <= self.len, "out of bounds");

        // SAFETY: cnt <= self.len
        unsafe {
            self.advance_unchecked(cnt);
        }
    }

    /// Advance [`Bytes`] to given pointer.
    ///
    /// # Examples
    ///
    /// This method is intended to be used with other API that returns a slice.
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// # fn find_space(b: &[u8]) -> &[u8] { &b[6..] }
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let world: &[u8] = find_space(bytes.as_slice());
    /// // SAFETY: `find_space` only returns slice within `bytes`
    /// unsafe {
    ///     bytes.advance_to_ptr(world.as_ptr())
    /// }
    /// assert_eq!(&bytes, &b"World!"[..]);
    /// ```
    ///
    /// # Safety
    ///
    /// - The distance between the pointers must be non-negative (`ptr >= self.ptr`)
    ///
    /// - *All* the safety conditions of pointer's `offset_from`
    ///   apply to this method as well; see it for the full details.
    #[inline]
    pub unsafe fn advance_to_ptr(&mut self, ptr: *const u8) {
        // SAFETY: caller ensure cnt <= self.len, and all `offset_from_unsigned
        unsafe {
            self.advance_unchecked(ptr.offset_from_unsigned(self.ptr.as_ptr()));
        }
    }

    pub(crate) unsafe fn advance_unchecked(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        debug_assert!(count <= self.len, "safety violated, out of bounds");

        let data = *self.data.get_mut();

        if let Ok(offset) = shared::as_unpromoted(data) {
            *self.data.get_mut() = shared::mask_payload(data, offset + count);
        }

        unsafe {
            self.ptr = self.ptr.add(count); // fn precondition
        }

        self.len -= count;
    }
}

// ===== Splitting =====

impl Bytes {
    /// Splits `Bytes` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `Bytes` contains
    /// elements `[at, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let split = bytes.split_off(6);
    /// assert_eq!(&bytes, &b"Hello "[..]);
    /// assert_eq!(&split, &b"World!"[..]);
    /// ```
    pub fn split_off(&mut self, at: usize) -> Self {
        let len = self.len;

        if at == len {
            // SAFETY: `self.ptr.add(self.len)` is always valid
            let ptr = unsafe { self.ptr.add(len) };
            return Bytes::new_empty_with_ptr(ptr);
        }

        if at == 0 {
            return mem::replace(self, Bytes::new_empty_with_ptr(self.ptr));
        }

        assert!(at <= len, "split_off out of bounds: {at:?} <= {len:?}");

        let mut clone = self.clone_inner();
        // SAFETY: `at <= self.len`
        unsafe { clone.advance_unchecked(at) };
        self.len = at;
        clone
    }

    /// Splits `Bytes` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `Bytes` contains
    /// elements `[0, at)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let split = bytes.split_to(6);
    /// assert_eq!(&bytes, &b"World!"[..]);
    /// assert_eq!(&split, &b"Hello "[..]);
    /// ```
    pub fn split_to(&mut self, at: usize) -> Self {
        let len = self.len;

        if at == len {
            // SAFETY: `self.ptr.add(self.len)` is valid
            let ptr = unsafe { self.ptr.add(len) };
            return mem::replace(self, Bytes::new_empty_with_ptr(ptr));
        }

        if at == 0 {
            return Bytes::new_empty_with_ptr(self.ptr);
        }

        assert!(at <= len, "split_to out of bounds: {at:?} <= {len:?}");

        let mut clone = self.clone_inner();
        // SAFETY: `at <= self.len`
        unsafe { self.advance_unchecked(at) };
        clone.len = at;
        clone
    }
}

// ===== Atomic Operations =====

impl Bytes {
    /// Returns `true` if `Bytes` is the only handle in a shared buffer.
    ///
    /// `Bytes` constructed from [`Bytes::from_static`] will always returns `false`.
    #[inline]
    pub fn is_unique(&self) -> bool {
        let shared = self.data.load(Ordering::Relaxed);

        if shared.is_null() {
            return false;
        }

        match shared::as_unpromoted(shared) {
            Ok(_) => true,
            Err(shared) => shared::is_unique(shared),
        }
    }

    fn clone_inner(&self) -> Self {
        let ptr = self.ptr;
        let len = self.len;
        let shared = self.data.load(Ordering::Relaxed);

        if shared.is_null() {
            return Self {
                ptr,
                len,
                data: AtomicPtr::new(std::ptr::null_mut()),
            };
        }

        match shared::as_unpromoted(shared) {
            Ok(offset) => {
                let vec = self.build_unpromoted_vec(offset);
                let new_shared = shared::promote_with_vec(vec, 2);

                // because cloning is called via the `Clone` trait, which take `&self`, and `Bytes`
                // is `Sync`, cloning could happens concurrently
                match self.data.compare_exchange(
                    shared,
                    new_shared,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(old_shared) => {
                        // the returned pointer is the old pointer
                        debug_assert!(std::ptr::eq(old_shared, shared));
                        debug_assert!(!std::ptr::eq(old_shared, new_shared));

                        Bytes {
                            ptr,
                            len,
                            data: AtomicPtr::new(new_shared),
                        }
                    }
                    Err(promoted_shared) => {
                        // concurrent promotion happens during heap allocation
                        debug_assert!(!std::ptr::eq(new_shared, promoted_shared));
                        // the written pointer should have been promoted
                        debug_assert!(shared::is_promoted(promoted_shared));

                        unsafe {
                            // release the heap that failed the promotion
                            shared::release(Box::from_raw(new_shared));

                            // increase the shared reference
                            shared::increment(&*promoted_shared);
                        }

                        Bytes {
                            ptr,
                            len,
                            data: AtomicPtr::new(promoted_shared),
                        }
                    }
                }
            }
            Err(shared_ref) => {
                shared::increment(shared_ref);
                Bytes {
                    ptr,
                    len,
                    data: AtomicPtr::new(shared),
                }
            }
        }
    }

    fn drop_inner(&mut self) {
        let shared = *self.data.get_mut();

        if shared.is_null() {
            return;
        }

        match shared::into_unpromoted(shared) {
            Ok(offset) => {
                let _ = self.build_unpromoted_vec(offset);
            }
            Err(shared) => {
                shared::release(shared);
            }
        }
    }

    /// Converts a [`Bytes`] into a byte vector.
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    pub fn into_vec(self) -> Vec<u8> {
        let mut bytes = ManuallyDrop::new(self);
        let shared = *bytes.data.get_mut();

        if shared.is_null() {
            return bytes.as_slice().to_vec();
        }

        let ptr = bytes.ptr.as_ptr();

        let (advanced, mut vec) = match shared::into_unpromoted(shared) {
            Ok(offset) => (offset, bytes.build_unpromoted_vec(offset)),
            Err(shared) => {
                let base_ptr = shared.as_ptr();
                let cap = shared.capacity();
                unsafe {
                    match shared::release_into_vec(shared, cap) {
                        Some(vec) => (ptr.offset_from_unsigned(base_ptr), vec),
                        None => {
                            // skip handling the `advance` below if we can directly copy
                            // the correct range
                            return bytes.as_slice().to_vec();
                        }
                    }
                }
            }
        };

        let len = bytes.len;

        if advanced != 0 {
            // `Bytes` has been `advanced`, `Vec` cannot represent that,
            // so we can only copy the buffer backwards
            unsafe {
                if advanced >= len {
                    ptr::copy_nonoverlapping(ptr, vec.as_mut_ptr(), len);
                } else {
                    ptr::copy(ptr, vec.as_mut_ptr(), len);
                }
            }
        }
        // we handle advancing to equal with `len`,
        // thus `len` bytes are initialized
        unsafe { vec.set_len(len) };
        vec
    }

    /// Converts a [`Bytes`] into a [`BytesMut`].
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    pub fn into_mut(self) -> BytesMut {
        let mut bytes = ManuallyDrop::new(self);
        let shared = *bytes.data.get_mut();

        if shared.is_null() {
            return BytesMut::from_vec(bytes.as_slice().to_vec());
        }

        let ptr = bytes.ptr.as_ptr();

        match shared::into_unpromoted(shared) {
            Ok(offset) => {
                let mut bufm = BytesMut::from_vec(bytes.build_unpromoted_vec(offset));
                unsafe {
                    // in contrast with `Vec`, `BytesMut` can represent `advance`,
                    // so no copying is required
                    bufm.advance_unchecked(offset);
                }
                bufm
            }
            Err(shared) => {
                let base_ptr = shared.as_ptr();
                let cap = shared.capacity();
                match unsafe { shared::release_into_vec(shared, cap) } {
                    Some(vec) => {
                        let mut bufm = BytesMut::from_vec(vec);
                        unsafe {
                            // handle head offset
                            bufm.advance_unchecked(ptr.offset_from_unsigned(base_ptr));
                            // handle tail offset
                            bufm.set_len(bytes.len);
                        }
                        bufm
                    }
                    None => BytesMut::from_vec(bytes.as_slice().to_vec()),
                }
            }
        }
    }

    fn build_unpromoted_vec(&self, offset: usize) -> Vec<u8> {
        unsafe {
            let base_ptr = self.ptr.sub(offset).as_ptr();
            let len = self.len + offset;

            // unpromoted will not represent tail offset, it will be promoted beforehand,
            // thus it is the same as full length vector
            Vec::from_raw_parts(base_ptr, len, len)
        }
    }
}

// ===== std traits =====

impl Drop for Bytes {
    #[inline]
    fn drop(&mut self) {
        self.drop_inner();
    }
}

impl Default for Bytes {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Bytes {
    #[inline]
    fn clone(&self) -> Self {
        self.clone_inner()
    }
}

impl AsRef<[u8]> for Bytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::fmt::Debug for Bytes {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

crate::macros::from! {
    impl Bytes;
    fn from(value: &'static [u8]) { Self::from_static(value) }
    fn from(value: &'static str) { Self::from_static(value.as_bytes()) }
    fn from(value: Box<[u8]>) { Self::from_box(value) }
    fn from(value: Vec<u8>) { Self::from_vec(value) }
    fn from(value: String) { Self::from_vec(value.into_bytes()) }
    fn from(value: BytesMut) { value.freeze() }
}

impl From<Bytes> for Vec<u8> {
    #[inline]
    fn from(value: Bytes) -> Self {
        value.into_vec()
    }
}

impl Eq for Bytes {}

crate::macros::partial_eq! {
    impl Bytes;
    fn eq(self, other: [u8]) { <[u8]>::eq(self, other) }
    fn eq(self, other: str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(self, other: Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(self, other: Self) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(self, other: BytesMut) { <[u8]>::eq(self, other.as_slice()) }
}
