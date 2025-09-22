use std::{
    mem::{self, ManuallyDrop},
    ptr, slice,
    sync::atomic::{AtomicPtr, Ordering},
};

use super::{
    Buf, BytesMut, Cursor, CursorBuf,
    shared::{self, Shared},
};

/// A cheaply cloneable and sliceable chunk of contiguous memory.
pub struct Bytes {
    ptr: *const u8,
    len: usize,
    /// it is requires to be atomic,
    /// buffer promotion requires to update the ptr
    data: AtomicPtr<()>,
    vtable: &'static Vtable,
}

unsafe impl Send for Bytes {}
unsafe impl Sync for Bytes {}

impl Bytes {
    /// Create new empty [`Bytes`].
    #[inline]
    pub const fn new() -> Self {
        Self::from_static(&[])
    }

    /// Create new [`Bytes`] from static slice.
    ///
    /// Since slice have static lifetime, cloning `Bytes` returned from this function is
    /// effectively a noop.
    ///
    /// Additionally, [`is_unique`][Bytes::is_unique] will always returns `false`.
    #[inline]
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
            data: AtomicPtr::new(ptr::null_mut()),
            vtable: Vtable::static_bytes(),
        }
    }

    /// Create new [`Bytes`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(data: &[u8]) -> Self {
        Self::from_vec(data.to_vec())
    }

    pub(crate) fn from_vec(mut vec: Vec<u8>) -> Self {
        let ptr = vec.as_mut_ptr();
        let len = vec.len();
        let cap = vec.capacity();

        // `into_boxed_slice`, which call `shrink_to_fit` will only reallocate
        // if `capacity > len`
        //
        // new created vector, the freezed returns from `BytesMut::split`
        // and `BytesMut::split_to` will trigger this branch
        if cap == len {
            return Self::from_box(vec.into_boxed_slice());
        }

        // PERF: we cannot start in unpromoted for `Shared` storage
        // Problems:
        // - we have nowhere to store capacity of the vector
        // - the `data` field already contains original pointer
        //   in case of `advance` which will change `ptr`
        // - if `len < cap`, there is a "tail offset", thus
        //   `len` cannot be calculated as capacity
        // Consideration:
        // - `into_boxed_slice`: reallocate and copy the bytes, as expensive as vector length
        // - `shared::promote_with_vec`: allocate `AtomicUsize`, pointer, and capacity (3 word)

        let shared = shared::promote_with_vec(vec, 1);

        Bytes {
            ptr,
            len,
            data: AtomicPtr::new(shared.cast()),
            vtable: Vtable::shared_promoted(),
        }
    }

    fn from_box(boxed: Box<[u8]>) -> Self {
        let len = boxed.len();
        let ptr = Box::into_raw(boxed).cast();

        let (data, vtable) = Vtable::shared_unpromoted(ptr);

        Bytes {
            ptr,
            len,
            data: AtomicPtr::new(data.cast()),
            vtable,
        }
    }

    pub(crate) fn from_mut(shared: *mut Shared, mut bytesm: BytesMut) -> Self {
        debug_assert!(shared::is_promoted(shared));
        Bytes {
            ptr: bytesm.as_mut_ptr(),
            len: bytesm.len(),
            data: AtomicPtr::new(shared.cast()),
            vtable: Vtable::shared_promoted(),
        }
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

    /// Returns `true` if `Bytes` is the only handle in a shared buffer.
    ///
    /// `Bytes` constructed from [`Bytes::from_static`] will always returns `false`.
    #[inline]
    pub fn is_unique(&self) -> bool {
        unsafe { (self.vtable.is_unique)(&self.data) }
    }

    /// Extracts a slice containing the entire bytes.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Returns a raw pointer to the buffer, or a dangling raw pointer valid for zero sized reads
    /// if the buffer didn't allocate.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Converts a [`Bytes`] into a byte vector.
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    #[inline]
    pub fn into_vec(self) -> Vec<u8> {
        let mut mem = ManuallyDrop::new(self);
        let me = &mut *mem;
        unsafe { (me.vtable.into_vec)(&mut me.data, me.ptr, me.len) }
    }

    /// Converts a [`Bytes`] into a [`BytesMut`].
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    #[inline]
    pub fn into_mut(self) -> BytesMut {
        let mut mem = ManuallyDrop::new(self);
        let me = &mut *mem;
        unsafe { (me.vtable.into_mut)(&mut me.data, me.ptr, me.len) }
    }

    /// Try to convert [`Bytes`] into [`BytesMut`] if its unique.
    #[inline]
    pub fn try_into_mut(self) -> Result<BytesMut, Self> {
        if self.is_unique() {
            Ok(self.into_mut())
        } else {
            Err(self)
        }
    }


    // private

    /// Specialized empty `Bytes` with given pointer.
    ///
    /// This is used when split and resulting in empty `Bytes` that does not need to increment the
    /// atomic counter.
    fn new_empty_with_ptr(ptr: *const u8) -> Self {
        Bytes {
            ptr: ptr::without_provenance(ptr.addr()),
            len: 0,
            data: AtomicPtr::new(ptr::null_mut()),
            vtable: Vtable::static_bytes(),
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub(crate) fn data(&self) -> &AtomicPtr<()> {
        &self.data
    }
}

impl Bytes {
    // ===== Read =====

    /// Advance [`Bytes`] to given pointer.
    ///
    /// # Examples
    ///
    /// This method is intended to be used with other API that returns a slice.
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// # fn find_delimiter(b: &[u8]) -> &[u8] { &b[9..] }
    /// let mut bytes = Bytes::copy_from_slice(b"userinfo@example.com");
    /// let host: &[u8] = find_delimiter(bytes.as_slice());
    /// // SAFETY: `find_delimiter` only returns slice within `bytes`
    /// unsafe {
    ///     bytes.advance_to_ptr(host.as_ptr())
    /// }
    /// assert_eq!(&bytes, &b"example.com"[..]);
    /// ```
    ///
    /// # Safety
    ///
    /// - The distance between the pointers must be non-negative (`ptr >= self.ptr`)
    ///
    /// - *All* the safety conditions of pointer's `offset_from`
    ///   apply to this method as well; see it for the full details.
    #[inline]
    pub const unsafe fn advance_to_ptr(&mut self, ptr: *const u8) {
        // SAFETY: caller ensure cnt <= self.len, and all `offset_from_unsigned
        unsafe {
            self.advance_unchecked(ptr.offset_from_unsigned(self.ptr));
        }
    }

    /// Shortens the buffer, keeping the first `len` bytes and dropping the rest.
    ///
    /// If `len` is greater or equal to the `BytesMut` length, this has no effect.
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
        if len < self.len {
            if Vtable::is_shared(self.vtable) {
                // this introduce "tail offset",
                // which cannot be represented in unpromoted,
                // thus required to be promoted
                drop(self.split_off(len));
            } else {
                self.len = len;
            }
        }
    }

    /// Shortens the buffer, dropping the last `len` bytes and keeping the rest.
    ///
    /// If `off` is greater to the `BytesMut` length, this has no effect.
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
        if let Some(new_len) = self.len.checked_sub(off) {
            if Vtable::is_shared(self.vtable) {
                // this introduce "tail offset",
                // which cannot be represented in unpromoted,
                // thus required to be promoted
                drop(self.split_off(new_len));
            } else {
                self.len = new_len;
            }
        }
    }

    /// Clears the buffer, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
    }

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

        assert!(end <= self_len, "range end out of bounds: {end:?} <= {self_len:?}",);

        // ASSERT:
        // 1. is `begin <= end`
        // 2. is `len <= end`
        // 3. is `end <= self.len`,
        // 4. #1, then `begin <= self.len`
        // 5. #1 and #2, then `len <= self.len`
        let len = end.checked_sub(begin).expect("range should not be reversed");

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

        let mut cloned = self.clone();
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
    #[inline]
    pub fn slice_ref(&self, subset: &[u8]) -> Self {
        self.slice_from_raw(subset.as_ptr(), subset.len())
    }

    /// Returns the shared subset of `Bytes` with given slice.
    ///
    /// # Panics
    ///
    /// The slice from `data` up to `len` should be contained in `Bytes` content, otherwise panic.
    pub fn slice_from_raw(&self, data: *const u8, len: usize) -> Self {
        let self_addr = self.ptr.addr();
        let addr = data.addr();

        // this checks that input end pointer is still within buffer range
        assert!(
            addr.checked_add(len).unwrap() <= self_addr + self.len,
            "length out of bounds"
        );

        let offset = addr
            .checked_sub(self_addr)
            .expect("pointer out of bounds");

        // SAFETY: this is the same as input `data` just using
        // usize offset to detach pointer provenance
        let data = unsafe { self.ptr.add(offset) };

        if len == 0 {
            return Self::new_empty_with_ptr(data);
        }

        // with assert and checked sub,
        // `data` is valid until `len` byte forward

        let mut cloned = self.clone();
        cloned.ptr = data;
        cloned.len = len;
        cloned
    }

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
    /// let mut bytes = Bytes::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.split_off(8);
    /// assert_eq!(&bytes, &b"userinfo"[..]);
    /// assert_eq!(&split, &b"@example.com"[..]);
    /// ```
    pub fn split_off(&mut self, at: usize) -> Self {
        let len = self.len;

        if at == len {
            // SAFETY: `self.ptr.add(self.len)` is valid
            let ptr = unsafe { self.ptr.add(len) };
            return Bytes::new_empty_with_ptr(ptr);
        }

        if at == 0 {
            return mem::replace(self, Bytes::new_empty_with_ptr(self.ptr));
        }

        assert!(at <= len, "split_off out of bounds: {at:?} <= {len:?}");

        let mut clone = self.clone();
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
    /// let mut bytes = Bytes::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.split_to(8);
    /// assert_eq!(&split, &b"userinfo"[..]);
    /// assert_eq!(&bytes, &b"@example.com"[..]);
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

        let mut clone = self.clone();
        // SAFETY: `at <= self.len`
        unsafe { self.advance_unchecked(at) };
        clone.len = at;
        clone
    }

    /// Create new [`CursorBuf`] from current `Bytes`.
    #[inline]
    pub const fn cursor(&self) -> Cursor<'_> {
        Cursor::new(self.as_slice())
    }

    /// Create new mutable [`CursorBuf`] from current buffer.
    #[inline]
    pub const fn cursor_mut(&mut self) -> CursorBuf<&mut Self> {
        CursorBuf::<&mut Self>::shared_mut(self)
    }


    // private

    /// # Safety
    ///
    /// `count <= self.len`
    #[inline]
    pub(crate) const unsafe fn advance_unchecked(&mut self, count: usize) {
        debug_assert!(count <= self.len, "safety violated, advanced out of bounds");
        self.len -= count;
        self.ptr = unsafe { self.ptr.add(count) };
    }
}

impl Buf for Bytes {
    #[inline]
    fn remaining(&self) -> usize {
        self.len
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.as_slice()
    }

    #[inline]
    fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.len,
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.len,
        );

        // SAFETY: cnt <= self.len
        unsafe {
            self.advance_unchecked(cnt);
        }
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> Bytes {
        self.split_to(len)
    }
}

// ===== Vtable =====

struct Vtable {
    /// fn(data, ptr, len)
    pub clone: unsafe fn(&AtomicPtr<()>, *const u8, usize) -> Bytes,
    /// fn(data)
    pub is_unique: unsafe fn(&AtomicPtr<()>) -> bool,
    /// fn(data, ptr, len)
    ///
    /// `into_*` consumes the `Bytes`, returning the respective value.
    pub into_vec: unsafe fn(&mut AtomicPtr<()>, *const u8, usize) -> Vec<u8>,
    pub into_mut: unsafe fn(&mut AtomicPtr<()>, *const u8, usize) -> BytesMut,
    /// fn(data, ptr, len)
    pub drop: unsafe fn(&mut AtomicPtr<()>, *const u8, usize),
}

// ===== Static Vtable =====

mod static_vtable {
    use super::*;

    impl Vtable {
        pub(super) const fn static_bytes() -> &'static Vtable {
            &STATIC_VTABLE
        }
    }

    const STATIC_VTABLE: Vtable = Vtable {
        clone: static_clone,
        into_vec: static_into_vec,
        into_mut: static_into_mut,
        is_unique: static_is_unique,
        drop: static_drop,
    };

    unsafe fn static_clone(_: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Bytes {
        unsafe { Bytes::from_static(slice::from_raw_parts(ptr, len)) }
    }

    fn static_is_unique(_: &AtomicPtr<()>) -> bool {
        false
    }

    unsafe fn static_into_vec(_: &mut AtomicPtr<()>, ptr: *const u8, len: usize) -> Vec<u8> {
        unsafe { slice::from_raw_parts(ptr, len).to_vec() }
    }

    unsafe fn static_into_mut(_: &mut AtomicPtr<()>, ptr: *const u8, len: usize) -> BytesMut {
        unsafe { BytesMut::from_vec(slice::from_raw_parts(ptr, len).to_vec()) }
    }

    unsafe fn static_drop(_: &mut AtomicPtr<()>, _: *const u8, _: usize) {
        // nothing to drop for &'static [u8]
    }
}

// ===== Shared Vtable =====

mod shared_vtable {
    use super::*;

    impl Vtable {
        pub(super) fn shared_unpromoted(data: *mut u8) -> (*mut u8, &'static Vtable) {
            if shared::is_payload_compliance(data.addr()) {
                (data, &SHARED)
            } else {
                // "map" the pointer to comply with `Shared` arbitrary payload
                //
                // but later, when it is used, it requires to be "map"-ed back
                (map_ptr(data), &MAPPED_SHARED)
            }
        }

        pub(super) const fn shared_promoted() -> &'static Vtable {
            // All shared vtable have the same behavior for promoted shared.
            &SHARED
        }

        pub(super) fn is_shared(vtable: &Vtable) -> bool {
            ptr::addr_eq(vtable, &SHARED) || ptr::addr_eq(vtable, &MAPPED_SHARED)
        }
    }


    fn noop(shared: *mut u8) -> *mut u8 {
        shared
    }

    fn map_ptr(shared: *mut u8) -> *mut u8 {
        shared.with_addr(!shared.addr())
    }

    macro_rules! with_map {
        ($fn_id:ident, $map_id:ident) => {
            |data, ptr, len| unsafe { $fn_id(data, ptr, len, $map_id) }
        };
    }

    static SHARED: Vtable = Vtable {
        clone: with_map!(clone, noop),
        is_unique,
        into_vec: with_map!(into_vec, noop),
        into_mut: with_map!(into_mut, noop),
        drop: with_map!(drop, noop),
    };

    /// Represent `Shared` with even pointer, that **not** comply with `Shared` arbitrary payload
    /// requirements. Therefore, it is required to map the pointer before retrieving the stored
    /// pointer.
    static MAPPED_SHARED: Vtable = Vtable {
        clone: with_map!(clone, map_ptr),
        is_unique,
        into_vec: with_map!(into_vec, map_ptr),
        into_mut: with_map!(into_mut, map_ptr),
        drop: with_map!(drop, map_ptr),
    };

    // NOTE:
    // in shared Vtable, the atomic pointer is `*mut Shared`,
    // and the arbitrary payload contains the buffer original pointer

    unsafe fn clone(
        data: &AtomicPtr<()>,
        ptr: *const u8,
        len: usize,
        map_ptr: fn(*mut u8) -> *mut u8,
    ) -> Bytes {
        let shared = data.load(Ordering::Relaxed).cast();

        match shared::as_unpromoted_raw(shared) {
            Ok(shared) => {
                // unpromoted will not represent tail offset, it will be promoted beforehand,
                // thus it is the same as full length vector
                let vec = unsafe { Vec::from_raw_parts(map_ptr(shared.cast()), len, len) };
                let new_shared = shared::promote_with_vec(vec, 2);

                // because cloning is called via the `Clone` trait, which take `&self`, and `Bytes`
                // is `Sync`, cloning could happens concurrently
                match data.compare_exchange(shared.cast(), new_shared.cast(), Ordering::AcqRel, Ordering::Acquire) {
                    Ok(old_shared) => {
                        // the returned pointer is the old pointer
                        debug_assert!(std::ptr::eq(old_shared, shared.cast()));
                        debug_assert!(!std::ptr::eq(old_shared, new_shared.cast()));

                        Bytes {
                            ptr,
                            len,
                            data: AtomicPtr::new(new_shared.cast()),
                            vtable: Vtable::shared_promoted(),
                        }
                    },
                    Err(promoted_shared) => {
                        // concurrent promotion happens during heap allocation
                        debug_assert!(!std::ptr::eq(new_shared, promoted_shared.cast()));
                        // the written pointer should have been promoted
                        assert!(shared::is_promoted(promoted_shared.cast()));

                        unsafe {
                            // release the heap that failed the promotion
                            shared::release(Box::from_raw(new_shared));

                            // increase the shared reference
                            shared::increment(&*promoted_shared.cast());
                        }

                        Bytes {
                            ptr,
                            len,
                            data: AtomicPtr::new(shared.cast()),
                            vtable: Vtable::shared_promoted(),
                        }
                    },
                }
            }
            Err(shared_ref) => {
                shared::increment(shared_ref);
                Bytes {
                    ptr,
                    len,
                    data: AtomicPtr::new(shared.cast()),
                    vtable: Vtable::shared_promoted(),
                }
            },
        }
    }

    unsafe fn is_unique(data: &AtomicPtr<()>) -> bool {
        let shared = data.load(Ordering::Relaxed).cast();

        match shared::as_unpromoted(shared) {
            Ok(_) => true,
            Err(shared) => shared::is_unique(shared),
        }
    }

    unsafe fn into_vec(
        data: &mut AtomicPtr<()>,
        ptr: *const u8,
        len: usize,
        map_ptr: fn(*mut u8) -> *mut u8,
    ) -> Vec<u8> {
        let shared = data.get_mut().cast();

        let (advanced, mut vec) = match shared::into_unpromoted_raw(shared) {
            Ok(buf_ptr) => {
                let buf_ptr = map_ptr(buf_ptr.cast());
                let advanced = unsafe { ptr.offset_from_unsigned(buf_ptr) };

                // unpromoted will not represent tail offset, it will be promoted beforehand,
                // thus it is the same as full length vector
                let vec = unsafe { Vec::from_raw_parts(buf_ptr, len, advanced + len) };

                (advanced, vec)
            }
            Err(shared) => {
                let buf_ptr = shared.as_ptr();
                let advanced = unsafe { ptr.offset_from_unsigned(buf_ptr) };
                let cap = shared.capacity();

                // the returned `Some(vec)` here can contains uninitialized bytes,
                // but it is fixed below
                match unsafe { shared::release_into_vec(shared, cap) } {
                    Some(vec) => (advanced, vec),
                    None => {
                        // skip handling the `advance` below if we can directly copy
                        // the correct range
                        return unsafe { slice::from_raw_parts(ptr, len).to_vec() }
                    },
                }
            }
        };

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

    unsafe fn into_mut(
        data: &mut AtomicPtr<()>,
        ptr: *const u8,
        len: usize,
        map_ptr: fn(*mut u8) -> *mut u8,
    ) -> BytesMut {
        let shared = data.get_mut().cast();

        match shared::into_unpromoted_raw(shared) {
            Ok(buf_ptr) => {
                let buf_ptr = map_ptr(buf_ptr.cast());
                let advanced = unsafe { ptr.offset_from_unsigned(buf_ptr) };

                // unpromoted will not represent tail offset, it will be promoted beforehand,
                // thus it is the same as full length vector
                //
                // we also add `advanced` to the total length,
                // so that `advance_unchecked()` below works correctly
                let vec = unsafe { Vec::from_raw_parts(buf_ptr, advanced + len, advanced + len) };
                let mut bufm = BytesMut::from_vec(vec);

                unsafe {
                    // in contrast with `Vec`, `BytesMut` can represent `advance`,
                    // so no copying is required
                    bufm.advance_unchecked(advanced);
                }
                assert_eq!(bufm.len(), len);
                bufm
            }
            Err(shared) => {
                let buf_ptr = shared.as_ptr();
                let offset = unsafe { ptr.offset_from_unsigned(buf_ptr) };
                let cap = shared.capacity();

                // the returned `Some(vec)` here can contains uninitialized bytes,
                // but it is fixed by `advance_unchecked` and `set_len`
                match unsafe { shared::release_into_vec(shared, cap) } {
                    Some(vec) => {
                        let mut bufm = BytesMut::from_vec(vec);
                        unsafe {
                            // handle head advanced
                            bufm.advance_unchecked(offset);
                            // handle tail offset
                            bufm.set_len(len);
                        }
                        bufm
                    }
                    None => BytesMut::from_vec(unsafe {
                        slice::from_raw_parts(ptr, len).to_vec()
                    }),
                }
            },
        }
    }

    unsafe fn drop(
        data: &mut AtomicPtr<()>,
        ptr: *const u8,
        len: usize,
        map_ptr: fn(*mut u8) -> *mut u8,
    ) {
        let shared = data.get_mut().cast();

        match shared::into_unpromoted_raw(shared) {
            Ok(buf_ptr) => {
                let buf_ptr = map_ptr(buf_ptr.cast());
                let advanced = unsafe { ptr.offset_from_unsigned(buf_ptr) };

                // unpromoted will not represent tail offset, it will be promoted beforehand,
                // thus it is the same as full length vector
                let _ = unsafe { Vec::from_raw_parts(buf_ptr, 0, advanced + len) };
            }
            Err(shared) => {
                shared::release(shared);
            }
        }
    }
}

// ===== std traits =====

impl Drop for Bytes {
    #[inline]
    fn drop(&mut self) {
        unsafe { (self.vtable.drop)(&mut self.data, self.ptr, self.len) }
    }
}

impl Clone for Bytes {
    #[inline]
    fn clone(&self) -> Self {
        unsafe { (self.vtable.clone)(&self.data, self.ptr, self.len) }
    }
}

impl std::fmt::Debug for Bytes {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }
}

impl Default for Bytes {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for Bytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
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

