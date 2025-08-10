use std::{
    mem::{self, ManuallyDrop},
    ptr, slice,
    sync::atomic::{AtomicPtr, Ordering},
};

use super::{
    Buf, BytesMut,
    shared,
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

    pub(crate) fn from_mut(shared: *mut shared::Shared, mut bytesm: BytesMut) -> Self {
        debug_assert!(shared::is_promoted(shared));
        Bytes {
            ptr: bytesm.as_mut_ptr(),
            len: bytesm.len(),
            data: AtomicPtr::new(shared as _),
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

    /// Shortens the buffer, keeping the first `len` bytes and dropping the rest.
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

    /// Clears the buffer, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
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

    fn new_empty_with_ptr(ptr: *const u8) -> Self {
        debug_assert!(!ptr.is_null());

        // Detach this pointer's provenance from whichever allocation it came from, and reattach it
        // to the provenance of the fake ZST [u8;0] at the same address.
        let ptr = ptr::without_provenance(ptr as usize);

        Bytes {
            ptr,
            len: 0,
            data: AtomicPtr::new(ptr::null_mut()),
            vtable: Vtable::static_bytes(),
        }
    }

    #[cfg(test)]
    pub(crate) fn data(&self) -> &AtomicPtr<()> {
        &self.data
    }

    /// # Safety
    ///
    /// `count <= self.len`
    #[inline]
    const unsafe fn advance_unchecked(&mut self, count: usize) {
        debug_assert!(count <= self.len, "Bytes::advance_unchecked out of bounds");
        self.len -= count;
        self.ptr = unsafe { self.ptr.add(count) };
    }
}

impl Bytes {
    // ===== Read =====

    /// Returns the shared subset of `Bytes` with given range.
    ///
    /// # Panics
    ///
    /// `range` should be in bounds of bytes capacity, otherwise panic.
    #[inline]
    pub fn slice(&self, range: impl core::ops::RangeBounds<usize>) -> Self {
        self.slice_inner(range.start_bound(), range.end_bound())
    }

    fn slice_inner(
        &self,
        start_bound: core::ops::Bound<&usize>,
        end_bound: core::ops::Bound<&usize>,
    ) -> Self {
        use core::ops::Bound;

        let len = self.len();

        let begin = match start_bound {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of range"),
            Bound::Unbounded => 0,
        };

        let end = match end_bound {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => len,
        };

        assert!(
            begin <= end,
            "range start must not be greater than end: {begin:?} <= {end:?}",
        );
        assert!(end <= len, "range end out of bounds: {end:?} <= {len:?}",);

        if end == begin {
            return Bytes::new_empty_with_ptr(self.ptr.wrapping_add(begin));
        }

        let mut cloned = self.clone();
        cloned.len = end - begin;
        cloned.ptr = unsafe { cloned.ptr.add(begin) };
        cloned
    }

    /// Returns the shared subset of `Bytes` with given slice.
    ///
    /// # Panics
    ///
    /// `subset` should be in bytes content, otherwise panic.
    pub fn slice_ref(&self, subset: &[u8]) -> Self {
        // Empty slice and empty Bytes may have their pointers reset
        // so explicitly allow empty slice to be a subslice of any slice.
        if subset.is_empty() {
            return Bytes::new();
        }

        let ptr = self.as_ptr() as usize;
        let len = self.len();

        let sub_p = subset.as_ptr() as usize;
        let sub_len = subset.len();

        assert!(
            sub_p >= ptr,
            "subset pointer ({:p}) is smaller than self pointer ({:p})",
            subset.as_ptr(),
            self.as_ptr(),
        );
        assert!(
            sub_p + sub_len <= ptr + len,
            "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
            self.as_ptr(),
            len,
            subset.as_ptr(),
            sub_len,
        );

        let sub_offset = sub_p - ptr;

        self.slice(sub_offset..(sub_offset + sub_len))
    }

    /// Splits `Bytes` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `Bytes` contains
    /// elements `[at, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    pub fn split_off(&mut self, at: usize) -> Self {
        if at == self.len() {
            return Bytes::new_empty_with_ptr(unsafe { self.ptr.add(at) });
        }

        if at == 0 {
            return mem::replace(self, Bytes::new_empty_with_ptr(self.ptr));
        }

        assert!(
            at <= self.len(),
            "split_off out of bounds: {:?} <= {:?}",
            at,
            self.len(),
        );

        let mut clone = self.clone();
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
    pub fn split_to(&mut self, at: usize) -> Self {
        if at == self.len() {
            return mem::replace(self, Bytes::new_empty_with_ptr(unsafe { self.ptr.add(at) }));
        }

        if at == 0 {
            return Bytes::new_empty_with_ptr(self.ptr);
        }

        assert!(
            at <= self.len,
            "split_to out of bounds: {:?} <= {:?}",
            at,
            self.len(),
        );

        let mut clone = self.clone();

        unsafe { self.advance_unchecked(at) };

        clone.len = at;
        clone
    }
}

crate::macros::impl_std_traits! {
    impl Bytes;

    fn drop(&mut self) {
        unsafe { (self.vtable.drop)(&mut self.data, self.ptr, self.len) }
    }

    fn clone(&self) {
        unsafe { (self.vtable.clone)(&self.data, self.ptr, self.len) }
    }

    fn fmt(&self: Bytes, f) {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }

    fn default() { Self::new() }
    fn deref(&self) -> &[u8] { self.as_slice() }

    fn from(value: &'static [u8]) { Self::from_static(value) }
    fn from(value: &'static str) { Self::from_static(value.as_bytes()) }
    fn from(value: Vec<u8>) { Self::from_vec(value) }
    fn from(value: String) { Self::from_vec(value.into_bytes()) }
    fn from(value: Box<[u8]>) -> Self { Self::from_box(value) }

    fn eq(&self, &other: [u8]) { <[u8]>::eq(self, other) }
    fn eq(&self, &other: &[u8]) { <[u8]>::eq(self, *other) }
    fn eq(&self, &other: str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(&self, &other: &str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(&self, &other: Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(&self, &other: &Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(&self, &other: Bytes) { <[u8]>::eq(self, &**other) }
    fn eq(&self, &other: BytesMut) { <[u8]>::eq(self, &**other) }
}

crate::macros::impl_std_traits! {
    fn from(value: Bytes) -> Vec<u8> { value.into_vec() }
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
            if shared::is_payload_compliance(data as _) {
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
        shared.with_addr(!(shared as usize))
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

                // `unpromoted` means there is only one handle,
                // thus its impossible to have concurent promotion
                data.store(new_shared.cast(), Ordering::Relaxed);
                assert!(shared::is_promoted(new_shared));
                Bytes {
                    ptr,
                    len,
                    data: AtomicPtr::new(new_shared.cast()),
                    vtable: Vtable::shared_promoted(),
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
                let advanced = unsafe { ptr.offset_from(buf_ptr) } as usize;

                // unpromoted will not represent tail offset, it will be promoted beforehand,
                // thus it is the same as full length vector
                let vec = unsafe { Vec::from_raw_parts(buf_ptr, len, advanced + len) };

                (advanced, vec)
            }
            Err(shared) => {
                let buf_ptr = shared.as_ptr();
                let advanced = unsafe { ptr.offset_from(buf_ptr) } as usize;
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
                let advanced = unsafe { ptr.offset_from(buf_ptr) } as usize;

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
                let offset = unsafe { ptr.offset_from(buf_ptr) } as usize;
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
                let advanced = unsafe { ptr.offset_from(buf_ptr) } as usize;

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
