use std::cmp;
use std::mem::{ManuallyDrop, MaybeUninit};
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
    data: *mut Shared,
}

unsafe impl Send for BytesMut { }
unsafe impl Sync for BytesMut { }

impl BytesMut {
    /// Create new empty [`BytesMut`].
    ///
    /// This function does not allocate.
    #[inline]
    pub const fn new() -> Self {
        BytesMut::from_vec(Vec::new())
    }

    /// Create new empty [`BytesMut`] with at least specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        BytesMut::from_vec(Vec::with_capacity(capacity))
    }

    /// Create new [`BytesMut`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(slice: &[u8]) -> BytesMut {
        BytesMut::from_vec(slice.to_vec())
    }

    pub(crate) const fn from_vec(mut vec: Vec<u8>) -> BytesMut {
        let len = vec.len();
        let cap = vec.capacity();
        let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };
        // prevent heap deallocation
        let _vec = ManuallyDrop::new(vec);
        BytesMut {
            ptr,
            len,
            cap,
            data: shared::new_unpromoted(),
        }
    }

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

    /// Returns the remaining spare capacity of the `BytesMut` as a slice of `MaybeUninit<T>`.
    #[inline]
    pub const fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe {
            slice::from_raw_parts_mut(self.ptr.as_ptr().add(self.len).cast(), self.cap - self.len)
        }
    }


    // private

    /// Consume `self.data` into owned `Vec<u8>`.
    ///
    /// # Safety
    ///
    /// Ensure that nothing else uses the pointer after calling this function.
    unsafe fn original_buffer(&self, offset: usize) -> Vec<u8> {
        unsafe {
            Vec::from_raw_parts(
                self.ptr.as_ptr().sub(offset),
                self.len + offset,
                self.cap + offset,
            )
        }
    }
}

impl BytesMut {
    // ===== Allocation =====

    /// Reserves capacity for at least `additional` more bytes to be inserted.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        if self.cap - self.len >= additional {
            return;
        }

        let _ = self.reserve_inner(additional, true);
    }

    /// Try to reclaim additional capacity without allocating.
    #[inline]
    pub fn try_reclaim(&mut self, additional: usize) -> bool {
        if self.cap - self.len >= additional {
            return true;
        }

        self.reserve_inner(additional, false)
    }

    /// Try to reclaim all leftover capacity without allocating.
    #[inline]
    pub fn try_reclaim_full(&mut self) -> bool {
        let additional = match shared::as_unpromoted(self.data) {
            Ok(offset) => offset + (self.cap - self.len),
            Err(shared) => shared.capacity() - self.len,
        };

        self.reserve_inner(additional, false)
    }

    /// Try to gain capacity without allocation
    ///
    /// The explanation is at the top of this file
    fn reserve_inner(&mut self, additional: usize, allocate: bool) -> bool {
        if additional == 0 {
            return true;
        }

        assert!(additional + self.cap <= isize::MAX as _);

        let ptr = self.ptr.as_ptr();
        let len = self.len;

        match shared::as_unpromoted_mut(self.data) {
            Ok(offset) => {
                let remaining = offset + (self.cap - self.len);

                // Case 1, copy the data backwards
                if remaining >= additional && offset >= len {
                    unsafe {
                        let buf_ptr = ptr.sub(offset);

                        // `offset >= len` guarantee no overlap
                        ptr::copy_nonoverlapping(ptr, buf_ptr, len);

                        self.ptr = NonNull::new_unchecked(buf_ptr);
                        self.cap += offset;

                        // reset the `offset`
                        self.data = shared::mask_payload(self.data, 0);

                        return true;
                    }
                }

                if !allocate {
                    return false;
                }

                unsafe {
                    // follow `Vec::reserve` logic instead of `Vec::with_capacity`,
                    // `max(exponential, additional)`
                    let capacity = cmp::max(self.cap * 2, len + additional);

                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);

                    // drop the original buffer *after* copy
                    drop(self.original_buffer(offset));


                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();
                    // reset the `offset`
                    self.data = shared::mask_payload(self.data, 0);
                }

                true
            },
            Err(shared) => {
                if shared::is_unique(shared) {
                    let shared_ptr = shared.as_ptr();
                    // SAFETY: `ptr` is originated from `shared_ptr`, and the only `ptr` operation
                    // is addition through `.advance_unchecked()`
                    let offset = unsafe { ptr.offset_from_unsigned(shared_ptr) };

                    // reclaim the leftover tail capacity
                    {
                        let shared_cap = shared.capacity();
                        let remaining_tail = shared_cap - (self.cap + offset);
                        self.cap += remaining_tail;

                        // Case 2
                        if remaining_tail >= additional {
                            return true;
                        }
                    }

                    let remaining = offset + (self.cap - self.len);

                    // Case 1, copy the data backwards
                    if remaining >= additional && offset >= len {
                        unsafe {
                            // `offset >= len` guarantee no overlap
                            ptr::copy_nonoverlapping(ptr, shared_ptr, len);

                            // reset the `offset`
                            self.ptr = NonNull::new_unchecked(shared_ptr);
                            self.cap += offset;

                            return true;
                        }
                    }

                    // give up reclaiming, try to reallocate
                }

                if !allocate {
                    return false;
                }

                // reallocate

                unsafe {
                    // follow `Vec::reserve` logic instead of `Vec::with_capacity`
                    let capacity = cmp::max(self.cap * 2, len + additional);

                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);

                    // release the shared buffer *after* copy
                    // let old_shared = ptr::read(shared);
                    shared::release(Box::from_raw(shared));

                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();
                    self.data = shared::new_unpromoted();

                    true
                }
            }
        }
    }
}

impl BytesMut {
    // ===== Read =====

    /// Advance [`BytesMut`] to given pointer.
    ///
    /// # Examples
    ///
    /// This method is intended to be used with other API that returns a slice.
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// # fn find_delimiter(b: &[u8]) -> &[u8] { &b[9..] }
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
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
    pub unsafe fn advance_to_ptr(&mut self, ptr: *const u8) {
        // SAFETY: caller ensure cnt <= self.len, and all `offset_from_unsigned
        unsafe {
            self.advance_unchecked(ptr.offset_from_unsigned(self.ptr.as_ptr()));
        }
    }

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
    /// If `off` is greater or equal to the `BytesMut` length, this has no effect.
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
        if let Some(new_len) = self.len.checked_sub(off) {
            self.len = new_len;
        }
    }

    /// Clears the `BytesMut`, removing all bytes.
    #[inline]
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// Converts `self` into an immutable [`Bytes`].
    #[inline]
    pub fn freeze(self) -> Bytes {
        match shared::as_unpromoted(self.data) {
            Ok(offset) => unsafe {
                let vec = ManuallyDrop::new(self).original_buffer(offset);
                let mut bytes = Bytes::from_vec(vec);
                bytes.advance(offset);
                bytes
            },
            Err(_) => Bytes::from_mut(self.data, self),
        }
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
        let mut clone = self.shallow_clone();
        unsafe {
            // `at <= self.len`, and `self.len <= self.cap`
            self.advance_unchecked(at);
        }
        clone.cap = at;
        clone.len = at;
        Some(clone)
    }

    /// Splits `BytesMut` into two at the given pointer.
    ///
    /// Afterwards `self` contains elements `[ptr, ptr + len)`, and the returned `BytesMut` contains
    /// elements `[0, ptr)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let (lead, rest): (&[u8], &[u8]) = bytes.split_at(8);
    ///
    /// assert_eq!(lead, &b"userinfo"[..]);
    /// assert_eq!(rest, &b"@example.com"[..]);
    ///
    /// let lead: BytesMut = bytes.split_to_ptr(rest.as_ptr());
    ///
    /// assert_eq!(&lead, &b"userinfo"[..]);
    /// assert_eq!(&bytes, &b"@example.com"[..]);
    /// ```
    #[inline]
    pub fn split_to_ptr(&mut self, ptr: *const u8) -> BytesMut {
        match ptr.addr().checked_sub(self.ptr.addr().get()) {
            Some(at) => self.split_to(at),
            None => panic!("split out of bounds")
        }
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
        let mut other = self.shallow_clone();
        unsafe {
            // `at <= self.cap`
            other.advance_unchecked(at);
        }
        self.cap = at;
        self.len = cmp::min(self.len, at); // could advance pass `self.len`
        Some(other)
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, ptr)`, and the returned `BytesMut` contains
    /// elements `[ptr, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"userinfo@example.com");
    /// let (lead, rest): (&[u8], &[u8]) = bytes.split_at(8);
    /// assert_eq!(lead, &b"userinfo"[..]);
    /// assert_eq!(rest, &b"@example.com"[..]);
    ///
    /// let rest: BytesMut = bytes.split_off_ptr(rest.as_ptr());
    /// assert_eq!(&bytes, &b"userinfo"[..]);
    /// assert_eq!(&rest, &b"@example.com"[..]);
    /// ```
    #[inline]
    pub fn split_off_ptr(&mut self, ptr: *const u8) -> BytesMut {
        match ptr.addr().checked_sub(self.ptr.addr().get()) {
            Some(at) => self.split_off(at),
            None => panic!("BytesMut::split_off_ptr out of bounds")
        }
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

        if let Ok(offset) = shared::as_unpromoted(self.data) {
            self.data = shared::mask_payload(self.data, offset + count);

            debug_assert!(offset + count < isize::MAX as usize);
        }

        unsafe {
            self.ptr = self.ptr.add(count); // fn precondition
            self.len = self.len.saturating_sub(count); // could advance pass `self.len`
            self.cap = self.cap.unchecked_sub(count); // fn precondition
        }
    }

    fn shallow_clone(&mut self) -> Self {
        match shared::as_unpromoted(self.data) {
            Ok(offset) => {
                let vec = unsafe { self.original_buffer(offset) };
                self.data = shared::promote_with_vec(vec, 2);
                debug_assert!(shared::is_promoted(self.data));
            }
            Err(shared) => {
                shared::increment(shared);
            }
        }

        unsafe { ptr::read(self) }
    }
}

impl BytesMut {
    // ===== Write =====

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
    pub fn try_unsplit(&mut self, other: BytesMut) -> Result<(), BytesMut> {
        if other.capacity() == 0 {
            return Ok(());
        }

        let ptr = unsafe { self.ptr.as_ptr().add(self.len) };

        if ptr == other.ptr.as_ptr()
            && shared::is_promoted(self.data)
            && shared::is_promoted(other.data)
        {
            self.len += other.len;
            self.cap += other.cap;
            Ok(())
        } else {
            Err(other)
        }
    }
}

// ===== std traits =====

impl Drop for BytesMut {
    #[inline]
    fn drop(&mut self) {
        match shared::into_unpromoted(self.data) {
            Ok(offset) => {
                // SAFETY: to be drop
                unsafe { drop(self.original_buffer(offset)) };
            },
            Err(shared) => {
                shared::release(shared);
            },
        }
    }
}

impl Clone for BytesMut {
    #[inline]
    fn clone(&self) -> Self {
        Self::copy_from_slice(self.as_slice())
    }
}

impl std::fmt::Debug for BytesMut {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }
}

impl Default for BytesMut {
    #[inline]
    fn default() -> Self {
        Self::new()
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
    fn from(value: &[u8]) { BytesMut::from_vec(value.to_vec()) }
    fn from(value: &str) { BytesMut::from_vec(value.as_bytes().to_vec()) }
    fn from(value: Vec<u8>) { BytesMut::from_vec(value) }
    fn from(value: Bytes) { value.into_mut() }
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
