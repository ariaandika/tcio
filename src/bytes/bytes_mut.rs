use std::cmp;
use std::mem::{self, MaybeUninit};
use std::ptr::{self, NonNull};
use std::slice;

use crate::bytes::shared::{self, Shared};
use crate::bytes::{Buf, Bytes, UninitSlice};

// BytesMut is a unique `&mut [u8]` over a shared heap allocated `[u8]`
//
// (heap)  : [------u8------]
// BytesMut: [--u8--]
// BytesMut:         [--u8--]

// # lazy shared allocation
//
// BytesMut have an optimization where at the start it is created,
// no shared heap is allocated, it is in `Owned` state
//
// therefore, if the bytes is not splitted, no additional heap is ever allocated
//
// this is denoted by the `data` field's least significant bit:
// - if the LSB is set, it does not yet allocate, `data` is invalid pointer
// - if the LSB is unset, `data` is a valid pointer to the shared heap allocation
//
// this can be achieved because `Shared` have even number memory alignment,
// thus the pointer LSB is always unset
//
// when shared memory is required, BytesMut switched to `Shared` state,
// the `Shared` struct is allocated to handle the underlying buffer lifecycle

// # `advance`
//
// in `Owned` state, the rest of the `data` field bit represent the `advance` value of `BytesMut`,
// that is only `size_of::<usize>() - 1` bit
//
// this is sufficient, because allocated objects can never be larger than `isize::MAX` bytes

// # Capacity Reclaim
//
// since BytesMut keep track of the original buffer,
// it can "reclaim" back a leftover shared allocation,
// gaining capacity without allocation
//
// reclaiming will only be performed when BytesMut is in `Owned` state,
// or it is unique in `Shared` state, that is,
// when there is only one reference exists to the shared buffer
//
// Case 1
//
// (heap)  : [--------------]
// BytesMut:         [------] (before)
// BytesMut: [------________] (after)
//
// in this case, it attempt to copy the data backwards
//
// copying only performed if offset and data does not overlap
//
// (heap)  : [--------------]
// BytesMut:     [----------]
//
// so in this case it will not reclaim
//
// Case 2
//
// (heap)  : [--------------]
// BytesMut: [------]         (before)
// BytesMut: [------________] (after)
//
// in this case, reclaiming will always succeed
//
// Case 3
//
// (heap)  : [--------------]
// BytesMut:     [------]     (before)
// BytesMut: [------________] (after)
//
// in this case, it combine the logic from case 1 and 2

const _: [(); size_of::<usize>() * 4] = [(); size_of::<BytesMut>()];
const _: [(); size_of::<usize>() * 4] = [(); size_of::<Option<BytesMut>>()];

/// A unique reference to a contiguous slice of memory.
pub struct BytesMut {
    ptr: NonNull<u8>,
    len: usize,
    cap: usize,
    data: NonNull<Shared>,
}

unsafe impl Send for BytesMut { }
unsafe impl Sync for BytesMut { }

impl Drop for BytesMut {
    #[inline]
    fn drop(&mut self) {
        match shared::as_unpromoted(self.data.as_ptr()) {
            Some(offset) => {
                if self.cap != 0 {
                    shared::deallocate(self.ptr, self.cap, offset);
                }
            },
            None => shared::release(self.data),
        }
    }
}

// ===== Constructor =====

impl BytesMut {
    /// Create new empty [`BytesMut`].
    ///
    /// This function does not allocate.
    #[inline]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            data: shared::NEW_UNPROMOTED,
        }
    }

    /// Create new empty [`BytesMut`] with at least specified capacity.
    ///
    /// If `capacity` is zero, this method will not allocate.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::new();
        }
        Self {
            ptr: shared::allocate(capacity),
            len: 0,
            cap: capacity,
            data: shared::NEW_UNPROMOTED,
        }
    }

    /// Create new [`BytesMut`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(slice: &[u8]) -> Self {
        if slice.is_empty() {
            return Self::new();
        }
        Self {
            ptr: shared::allocate_copy(slice),
            len: slice.len(),
            cap: slice.len(),
            data: shared::NEW_UNPROMOTED,
        }
    }
}

impl Default for BytesMut {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for BytesMut {
    #[inline]
    fn clone(&self) -> Self {
        Self::copy_from_slice(self.as_slice())
    }
}

// ===== Getters =====

impl BytesMut {
    /// Returns the number of bytes in the `BytesMut`.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if `BytesMut` contains no bytes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the bytes that `BytesMut` can hold without reallocating.
    #[inline]
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// Returns the bytes as a shared slice.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Returns the bytes as a mutable slice.
    #[inline]
    pub const fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// Returns a raw pointer to the buffer, or a dangling raw pointer valid for zero sized reads
    /// if the buffer didn't allocate.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Returns a raw mutable pointer to the buffer, or a dangling raw pointer valid for zero sized
    /// reads if the buffer didn't allocate.
    #[inline]
    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Returns the remaining spare capacity of the `BytesMut` as a slice of `MaybeUninit<T>`.
    #[inline]
    pub const fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe {
            slice::from_raw_parts_mut(self.ptr.as_ptr().add(self.len).cast(), self.cap - self.len)
        }
    }

    // private

    /// (ptr, len, cap, data)
    pub(super) fn into_raw_parts(self) -> (NonNull<u8>, usize, usize, NonNull<Shared>) {
        let me = std::mem::ManuallyDrop::new(self);
        (me.ptr, me.len, me.cap, me.data)
    }
}

// ===== Allocation =====

impl BytesMut {
    /// Reserves capacity for at least `additional` more bytes to be inserted.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        assert!(self.cap.overflowing_add(additional).1);
        if additional == 0 {
            return;
        }
        if self.cap - self.len >= additional {
            return;
        }
        self.reserve_inner(additional);
    }

    /// Try to reclaim additional capacity without allocating.
    ///
    /// Returns `true` if reclaiming success without allocating.
    #[inline]
    pub fn try_reclaim(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        if self.cap - self.len >= additional {
            return;
        }
        if self.cap.checked_add(additional).is_none() {
            return;
        }
        self.reserve_inner(additional);
    }

    fn reserve_inner(&mut self, additional: usize) {
        let (base_raw, offset) = match shared::as_unpromoted_non_null(self.data) {
            Ok(offset) => (
                Some((unsafe { self.ptr.sub(offset) }, self.cap + offset)),
                offset,
            ),
            Err(shared) => {
                if shared::is_unique(shared) {
                    let base_raw = (shared.as_non_null(), shared.capacity());
                    let offset = unsafe { self.ptr.offset_from_unsigned(base_raw.0) };
                    // reclaim the leftover tail capacity
                    self.cap = base_raw.1 - offset;
                    (Some(base_raw), offset)
                } else {
                    (None, unsafe {
                        self.ptr.offset_from_unsigned(shared.as_non_null())
                    })
                }
            }
        };

        // copy the data backwards, only if its nonoverlapping and the buffer is exclusively owned
        let offset = if let Some((base_ptr, base_cap)) = base_raw
            && offset >= self.len
        {
            unsafe { ptr::copy_nonoverlapping(self.ptr.as_ptr(), base_ptr.as_ptr(), self.len) };
            self.ptr = base_ptr;
            self.cap = base_cap;
            if shared::is_unpromoted(self.data.as_ptr()) {
                self.data = shared::NEW_UNPROMOTED;
            }
            0
        } else {
            offset
        };

        if self.cap - self.len >= additional {
            // enough capacity without reallocating
            return;
        }

        // allocation
        match base_raw {
            Some((base_ptr, base_cap)) => {
                let new_cap = cmp::max(base_cap * 2, self.len + offset + additional);
                let new_ptr = if shared::is_unpromoted(self.data.as_ptr()) {
                    shared::grow(base_ptr, base_cap, new_cap)
                } else {
                    unsafe { self.data.as_mut().grow(new_cap) }
                };
                self.ptr = unsafe { new_ptr.add(offset) };
                self.cap = new_cap - offset;
            }
            None => {
                // the buffer is not exclusive, `shared::grow` cannot be used, new allocation is
                // required
                let base_cap = self.cap + offset;
                let new_cap = cmp::max(base_cap * 2, self.len + offset + additional);
                let new_base_ptr = shared::allocate(new_cap);
                shared::release(self.data);
                self.ptr = unsafe { new_base_ptr.add(offset) };
                self.cap = new_cap - offset;
            }
        }
    }
}

// ===== Read =====

impl BytesMut {
    /// Shortens the buffer, keeping the first `len` bytes and dropping the rest.
    ///
    /// If `len` is greater or equal to the `BytesMut` length, this has no effect.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// bytes.truncate(8);
    /// assert_eq!(bytes.as_slice(), b"userinfo");
    /// ```
    #[inline]
    pub const fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.len = len;
        }
    }

    /// Shortens the buffer, dropping the last `len` bytes and keeping the rest.
    ///
    /// If `off` is greater or equal to the `BytesMut` length, this will clear the bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// bytes.truncate_off(b"@example.com".len());
    /// assert_eq!(bytes.as_slice(), b"userinfo");
    /// ```
    #[inline]
    pub const fn truncate_off(&mut self, off: usize) {
        self.len = self.len.saturating_sub(off);
    }

    /// Clears the `BytesMut`, removing all bytes.
    #[inline]
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// Removes the bytes from the current view, returning them in a new `BytesMut` handle.
    ///
    /// Afterwards, `self` will be empty, but will retain any additional capacity that it had before
    /// the operation. This is identical to `self.split_to(self.len())`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.split();
    /// assert!(bytes.is_empty());
    /// assert_eq!(&split, &b"userinfo@example.com"[..]);
    /// ```
    #[inline]
    pub fn split(&mut self) -> BytesMut {
        self.split_to(self.len)
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `BytesMut` contains
    /// elements `[0, at)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.split_to(8);
    /// assert_eq!(&split, &b"userinfo"[..]);
    /// assert_eq!(&bytes, &b"@example.com"[..]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > self.len()`.
    #[inline]
    pub fn split_to(&mut self, at: usize) -> BytesMut {
        match self.try_split_to(at) {
            Some(ok) => ok,
            None => panic!("split_to out of bounds: {at:?} <= {:?}", self.len),
        }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `BytesMut` contains
    /// elements `[0, at)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// Returns `None` if `at > self.len()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// # fn run() -> Option<()> {
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.try_split_to(8)?;
    /// assert_eq!(&split, &b"userinfo"[..]);
    /// assert_eq!(&bytes, &b"@example.com"[..]);
    /// assert!(bytes.try_split_to(16).is_none());
    /// # Some(())
    /// # }
    /// # assert!(run().is_some());
    /// ```
    #[inline]
    pub fn try_split_to(&mut self, at: usize) -> Option<BytesMut> {
        if at > self.len {
            return None;
        }
        let clone = self.shallow_clone(at);
        self.len = at;
        self.cap = at;
        Some(mem::replace(self, clone))
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `BytesMut` contains
    /// elements `[at, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.split_off(8);
    /// assert_eq!(&bytes, &b"userinfo"[..]);
    /// assert_eq!(&split, &b"@example.com"[..]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > self.capacity()`.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> BytesMut {
        match self.try_split_off(at) {
            Some(ok) => ok,
            None => panic!("split_off out of bounds: {at:?} <= {:?}", self.len),
        }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `BytesMut` contains
    /// elements `[at, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// Returns `None` if `at > self.capacity()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// # fn run() -> Option<()> {
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let split = bytes.try_split_off(8)?;
    /// assert_eq!(&bytes, &b"userinfo"[..]);
    /// assert_eq!(&split, &b"@example.com"[..]);
    /// assert!(bytes.try_split_off(16).is_none());
    /// # Some(())
    /// # }
    /// # assert!(run().is_some());
    /// ```
    #[inline]
    pub fn try_split_off(&mut self, at: usize) -> Option<BytesMut> {
        if at > self.cap {
            return None;
        }
        let clone = self.shallow_clone(at);
        self.cap = at;
        self.len = cmp::min(self.len, at); // could split pass `self.len`
        Some(clone)
    }

    /// # Safety
    ///
    /// `count <= self.cap`
    pub(crate) unsafe fn advance_unchecked(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        debug_assert!(
            count <= self.cap,
            "BytesMut::advance_unchecked out of bounds"
        );

        if let Ok(offset) = shared::as_unpromoted_non_null(self.data) {
            // SAFETY: `self.data` is unpromoted
            self.data = shared::mask_payload(self.data.as_ptr(), offset + count);
        }

        self.ptr = unsafe { self.ptr.add(count) };
        self.len -= count;
        self.cap -= count;
    }

    fn shallow_clone(&mut self, at: usize) -> Self {
        match shared::as_unpromoted_non_null(self.data) {
            Ok(offset) => self.data = shared::promote_with(self.ptr, self.cap, offset, 2),
            Err(shared) => shared::increment(shared),
        }
        Self {
            ptr: unsafe { self.ptr.add(at) },
            len: self.len - at,
            cap: self.cap - at,
            data: self.data,
        }
    }
}

// ===== Write =====

impl BytesMut {
    /// Forces the length of the `BytesMut` to `new_len`.
    ///
    /// # Safety
    ///
    /// * `new_len` must be less than or equal to [`BytesMut::capacity()`].
    /// * The elements at `old_len..new_len` must be initialized.
    #[inline]
    pub const unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(new_len <= self.cap, "BytesMut::set_len out of bounds");
        self.len = new_len;
    }

    /// Copy and append bytes to the `BytesMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(&[1, 2, 3]);
    /// bytes.extend_from_slice(&[4, 5, 6]);
    /// assert_eq!(&bytes, &[1, 2, 3, 4, 5, 6])
    /// ```
    #[inline]
    pub fn extend_from_slice(&mut self, extend: &[u8]) {
        let additional = extend.len();
        self.reserve(additional);

        unsafe {
            let dst = self.spare_capacity_mut();

            debug_assert!(dst.len() >= additional);

            ptr::copy_nonoverlapping(extend.as_ptr(), dst.as_mut_ptr().cast(), additional);

            self.len += additional;
        }
    }

    /// Absorbs a `BytesMut` that was previously split off.
    ///
    /// If the two `BytesMut` were previously contiguous, this is an `O(1)` operation that just
    /// decrease a reference count and sets few indices.
    ///
    /// Otherwise, it copies and append the bytes to the current `BytesMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let ptr = bytes.as_ptr();
    /// let split = bytes.split_off(6);
    ///
    /// assert_eq!(&bytes, b"Hello ");
    /// assert_eq!(&split, b"World!");
    ///
    /// bytes.unsplit(split);
    /// assert_eq!(&bytes, &b"Hello World!"[..]);
    /// assert_eq!(ptr, bytes.as_ptr());
    /// ```
    #[inline]
    pub fn unsplit(&mut self, other: BytesMut) {
        if self.is_empty() {
            *self = other;
            return;
        }

        if let Err(other) = self.try_unsplit(other) {
            self.extend_from_slice(&other);
        }
    }

    /// Absorbs a `BytesMut` that was previously split off.
    ///
    /// If the two `BytesMut` were previously contiguous, this is an `O(1)` operation that just
    /// decrease a reference count, sets few indices and returns [`Ok`].
    ///
    /// Otherwise, it returns [`Err`] containing the same given `BytesMut`.
    #[inline]
    pub fn try_unsplit(&mut self, other: BytesMut) -> Result<(), BytesMut> {
        if other.capacity() == 0 {
            return Ok(());
        }

        let ptr = unsafe { self.ptr.add(self.len) };

        if ptr == other.ptr
            && shared::is_promoted(self.data.as_ptr())
            && shared::is_promoted(other.data.as_ptr())
        {
            self.len += other.len;
            self.cap += other.cap;
            Ok(())
        } else {
            Err(other)
        }
    }
}

// ===== Convertion =====

impl BytesMut {
    /// Converts `self` into an immutable [`Bytes`].
    #[inline]
    pub fn freeze(self) -> Bytes {
        Bytes::from(self)
    }
}

impl From<Box<[u8]>> for BytesMut {
    #[inline]
    fn from(value: Box<[u8]>) -> Self {
        let ptr = NonNull::new(Box::into_raw(value)).expect("box cannot be null");
        Self {
            ptr: ptr.cast(),
            len: ptr.len(),
            cap: ptr.len(),
            data: shared::NEW_UNPROMOTED,
        }
    }
}

impl From<Vec<u8>> for BytesMut {
    #[inline]
    fn from(value: Vec<u8>) -> Self {
        let (ptr, len, cap) = value.into_raw_parts();
        Self {
            ptr: NonNull::new(ptr).expect("vec cannot be null"),
            len,
            cap,
            data: shared::NEW_UNPROMOTED,
        }
    }
}

impl From<Bytes> for BytesMut {
    /// Converts a [`Bytes`] into a [`BytesMut`].
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    #[inline]
    fn from(value: Bytes) -> Self {
        let (ptr, len, data) = value.into_raw_parts();

        let Some(data) = NonNull::new(data) else {
            let slice = unsafe { slice::from_raw_parts(ptr.as_ptr(), len) };
            return Self::copy_from_slice(slice);
        };

        match shared::as_unpromoted_non_null(data) {
            Ok(_) => Self {
                ptr,
                len,
                cap: len,
                data,
            },
            Err(shared) => {
                // `BytesMut` requires guarantee that current buffer slice is unique from the
                // entire buffer
                if shared::is_unique(shared) {
                    Self {
                        ptr,
                        len,
                        cap: len,
                        data,
                    }
                } else {
                    shared::release(data);
                    let slice = unsafe { slice::from_raw_parts(ptr.as_ptr(), len) };
                    Self::copy_from_slice(slice)
                }
            }
        }
    }
}

impl From<BytesMut> for Box<[u8]> {
    #[inline]
    fn from(value: BytesMut) -> Self {
        Vec::from(value).into_boxed_slice()
    }
}

impl From<BytesMut> for Vec<u8> {
    fn from(value: BytesMut) -> Self {
        let (ptr, len, cap, data) = value.into_raw_parts();
        let ptr = ptr.as_ptr();
        let (base_ptr, base_cap) = match shared::as_unpromoted(data.as_ptr()) {
            Some(offset) => unsafe { (ptr.sub(offset), cap + offset) },
            None => match shared::release_into_raw(data) {
                Some((ptr, cap)) => (ptr.as_ptr(), cap),
                None => return unsafe { slice::from_raw_parts_mut(ptr, len) }.to_vec(),
            },
        };
        if ptr != base_ptr {
            // `BytesMut` has been `advanced`, but `Vec` cannot represent that
            //
            // thus we need to copy the bytes backwards
            unsafe {
                let offset = ptr.offset_from_unsigned(base_ptr);
                if offset > len {
                    ptr::copy_nonoverlapping(ptr, base_ptr, len)
                } else {
                    ptr::copy(ptr, base_ptr, len)
                }
            }
        }
        unsafe { Vec::from_raw_parts(base_ptr, len, base_cap) }
    }
}

// ===== std traits =====

impl std::fmt::Debug for BytesMut {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }
}

impl std::ops::Deref for BytesMut {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for BytesMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl AsRef<[u8]> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for BytesMut {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

crate::macros::from! {
    impl BytesMut;
    fn from(value: String) { BytesMut::from(value.into_bytes()) }
}

impl Eq for BytesMut {}

crate::macros::partial_eq! {
    impl BytesMut;
    fn eq(self, other: [u8]) { <[u8]>::eq(self, other) }
    fn eq(self, other: str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(self, other: Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(self, other: Self) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(self, other: Bytes) { <[u8]>::eq(self, other.as_slice()) }
}

impl<const N: usize> PartialEq<[u8; N]> for BytesMut {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_slice() == other
    }
}

impl crate::bytes::BufMut for BytesMut {
    #[inline]
    fn remaining_mut(&self) -> usize {
        isize::MAX as usize - self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut UninitSlice {
        if self.capacity() == self.len() {
            self.reserve(64);
        }
        UninitSlice::from_uninit(self.spare_capacity_mut())
    }

    #[inline]
    unsafe fn advance_mut(&mut self, cnt: usize) {
        unsafe { self.set_len(self.len() + cnt) };
    }

    fn put<T: Buf>(&mut self, mut src: T)
    where
        Self: Sized,
    {
        if !src.has_remaining() {
            // prevent calling `copy_to_bytes`->`put`->`copy_to_bytes` infintely when src is empty

        } else if self.capacity() == 0 {
            // When capacity is zero, try reusing allocation of `src`.
            let src_copy = src.copy_to_bytes(src.remaining());
            drop(src);
            if src_copy.is_unique() {
                *self = src_copy.into_mut();
            } else {
                self.extend_from_slice(&src_copy)
            }
        } else {
            self.reserve(src.remaining());
            while src.has_remaining() {
                let s = src.chunk();
                let l = s.len();
                self.extend_from_slice(s);
                src.advance(l);
            }
        }
    }

    #[inline]
    fn put_slice(&mut self, src: &[u8]) {
        self.extend_from_slice(src);
    }
}

impl std::io::Read for BytesMut {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = buf.len().min(self.len());
        buf[..read].copy_from_slice(&self[..read]);
        self.advance(read);
        Ok(read)
    }
}

impl std::io::Write for BytesMut {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
