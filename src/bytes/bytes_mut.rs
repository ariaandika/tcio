//! `BytesMut` internal documentation.
//!
//! # Memory Management
//!
//! `BytesMut` uses [`Shared`] for memory management, a regular heap allocation that can be
//! "promoted" to a reference counted allocation. In this docs, the terms "unpromoted" and
//! "promoted" are explained in the `Shared` documentation.
//!
//! Additionally, `BytesMut` have guarantee that the slice it contains are unique to the entire
//! allocation. In contrast with [`Bytes`], the slice will never overlap.
//!
//! ```not_rust
//! Shared  : [-------------]
//! Bytes   : [---]
//! Bytes   : [-----]
//! BytesMut:        [------]
//! ```
//!
//! This allows for mutable reference to the underlying slice. Available via
//! [`BytesMut::as_mut_slice`].
//!
//! # Advancing
//!
//! `BytesMut` implement [`Buf`] which represent a cursor to mark a read data.
//!
//! In "unpromoted" state, the offset is stored in the `data` field.
//!
//! In "promoted" state, the advance offset is the same as pointer offset from the beginning of the
//! allocation.
//!
//! # Capacity Reclaim
//!
//! In "unpromoted" state, `BytesMut` can be advanced, leaving the beginning of the allocation
//! unused. When reserving, `BytesMut` can "reclaim" back this leftover allocation, gaining
//! capacity without reallocating.
//!
//! Reclaiming in this state works by copying the initialized data backwards to the beginning of
//! the allocation. The copying is restricted to only use the `copy_nonoverlapping` function.
//! Therefore, reclaiming only happens if the backward copy will not overlap. In other words, the
//! offset length should be larger than the initialized data length.
//!
//! In "promoted" state, `BytesMut` can only reclaim allocation if its the only instance that
//! holds the `Shared` data. In addition to copying the initialized data backward, `BytesMut` can
//! also reclaim leftover allocation that are "ahead" of its slice.
//!
//! Case 1
//!
//! In the following case, it will copy the data backwards.
//!
//! ```not_rust
//! Shared  : [--------------]
//! BytesMut:         [------] (before)
//! BytesMut: [------________] (after reclaim)
//! ```
//!
//! If the copy will overlap, reclaim will not be performed.
//!
//! ```not_rust
//! Shared  : [--------------]
//! BytesMut:     [----------] (cannot reclaim)
//! ```
//!
//! Case 2
//!
//! In the following case, the leftover allocation will also be reclaimed.
//!
//! ```not_rust
//! Shared  : [--------------]
//! BytesMut: [------]         (before)
//! BytesMut: [------________] (after reclaim)
//! ```
//!
//! Case 3
//!
//! In the following case, it combine the logic from case 1 and 2.
//!
//! ```not_rust
//! Shared  : [--------------]
//! BytesMut:        [---]     (before)
//! BytesMut: [---___________] (after)
//! ```
use std::cmp;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::slice;

use crate::bytes::shared::{self, Shared};
use crate::bytes::{Bytes, UninitSlice};

const _: [(); size_of::<usize>() * 4] = [(); size_of::<BytesMut>()];
const _: [(); size_of::<usize>() * 4] = [(); size_of::<Option<BytesMut>>()];

/// A contiguous growable in memory buffer.
///
/// Semantically, this is similar to `Vec<u8>` with additional features.
///
/// # Usage
///
/// `BytesMut` are intended to be used in networking, where parsing bytes that involves splitting
/// bytes is a cheap operation.
///
/// ```
/// use tcio::bytes::{BytesMut, Buf};
///
/// let mut buffer = BytesMut::with_capacity(1024);
///
/// // pretend that we read data from TCP
/// buffer.extend_from_slice(b"GET / HTTP/1.1\r\n\r\n");
///
/// let at = buffer.iter().position(|&e|e == b' ').unwrap();
///
/// // this new instance does not allocate new memory
/// let method: BytesMut = buffer.split_to(at);
///
/// assert_eq!(&method, b"GET");
///
/// buffer.advance(1);
/// assert_eq!(&buffer, b"/ HTTP/1.1\r\n\r\n");
/// ```
///
/// # Capacity and reallocation
///
/// The capacity and reallocation behavior are very similar to the [`Vec`] type. See its struct
/// documentation for more details.
///
/// Additionally, `BytesMut` will allocate additonal small memory on demand, that upgrade the
/// buffer which allows the allocation to be shared among multiple instance of `Bytes` or
/// `BytesMut`. This makes bytes splitting a cheap operation.
///
/// # Splitting
///
/// Unlike `Vec::split_off`, [`BytesMut::split_off`] and [`BytesMut::split_to`] **will not**
/// allocate new memory. The new `BytesMut` instance will point to different slice of the memory,
/// but still uses the same allocation.
///
/// # [`Buf`]/[`BufMut`]
///
/// `BytesMut` implement `Buf` and `BufMut`. See its documentation for more details.
///
/// ```
/// # let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
/// use tcio::bytes::{BytesMut, Buf, BufMut};
///
/// let mut read_buf = BytesMut::copy_from_slice(&data);
/// let mut write_buf = BytesMut::with_capacity(read_buf.len());
///
/// while let Some(byte) = read_buf.try_get_u8() {
///     if byte.is_multiple_of(2) {
///         write_buf.put_u8(byte);
///     }
/// }
/// ```
///
/// [`BufMut`]: crate::bytes::BufMut
///
/// # Conversion
///
/// `BytesMut` can be created from `Vec<u8>` or `Box<[u8]>` via the [`From`] implementation. It
/// will just reuse the buffer without any copying or reallocation.
///
/// `BytesMut` can also be converted to `Vec<u8>` or `Box<[u8]>`. But in this case, it **may**
/// reuse the buffer if it can. Otherwise, a copy and allocation is required.
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
            Some(offset) => shared::deallocate(self.ptr, self.cap, offset),
            None => shared::release(self.data),
        }
    }
}

// ===== Constructor =====

impl BytesMut {
    /// Constructs a new, empty `BytesMut`.
    ///
    /// This method does not allocate.
    #[inline]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            data: shared::NEW_UNPROMOTED,
        }
    }

    /// Constructs a new, empty `BytesMut` with at least specified capacity.
    ///
    /// If `capacity` is zero, the buffer will not allocate.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity exceeds `isize::MAX` _bytes_.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity > isize::MAX as usize {
            shared::capacity_overflow()
        }
        let ptr = if capacity == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: `capacity` is `1..=isize::MAX`
            unsafe { shared::allocate(capacity) }
        };
        Self {
            ptr,
            len: 0,
            cap: capacity,
            data: shared::NEW_UNPROMOTED,
        }
    }

    /// Constructs new `BytesMut`, and copy the given bytes to the buffer.
    #[inline]
    pub fn copy_from_slice(slice: &[u8]) -> Self {
        let ptr = if slice.is_empty() {
            NonNull::new(slice.as_ptr().cast_mut()).expect("ref cannot be null")
        } else {
            unsafe {
                // SAFETY: `slice.len()` is `1..=isize::MAX`, no allocation can be larger than
                // `isize::MAX` bytes.
                let ptr = shared::allocate(slice.len());
                ptr.as_ptr().copy_from_nonoverlapping(slice.as_ptr(), slice.len());
                ptr
            }
        };
        Self {
            ptr,
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
        let len = self.len;
        // SAFETY: The maximum capacity is `isize::MAX` bytes
        unsafe { std::hint::assert_unchecked(len <= isize::MAX as usize) };
        len
    }

    /// Returns `true` if `BytesMut` contains no bytes.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total number of bytes that `BytesMut` can hold without reallocating.
    #[inline]
    pub const fn capacity(&self) -> usize {
        let cap = self.cap;
        // SAFETY: The maximum capacity is `isize::MAX` bytes
        unsafe { std::hint::assert_unchecked(cap <= isize::MAX as usize) };
        cap
    }

    /// Returns the bytes as a shared slice.
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }

    /// Returns the bytes as a mutable slice.
    #[inline]
    pub const fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
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

    /// Returns the remaining spare capacity of the `BytesMut` as a slice of `MaybeUninit<u8>`.
    ///
    /// The returned slice can be used to fill the buffer with data (e.g. by reading from a file)
    /// before marking the data as initialized using the [`set_len`] method.
    ///
    /// [`set_len`]: Self::set_len
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::with_capacity(10);
    ///
    /// let uninit = bytes.spare_capacity_mut();
    /// assert!(uninit.len() >= 10);
    ///
    /// // Fill in the first 3 elements.
    /// uninit[0].write(2);
    /// uninit[1].write(4);
    /// uninit[2].write(6);
    ///
    /// // Mark the first 3 elements of the vector as being initialized.
    /// unsafe { bytes.set_len(3) };
    ///
    /// assert_eq!(&bytes, &[2, 4, 6]);
    /// ```
    #[inline]
    pub const fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        unsafe {
            slice::from_raw_parts_mut(
                self.ptr.as_ptr().add(self.len()).cast(),
                self.capacity() - self.len()
            )
        }
    }

    // private

    /// (ptr, len, cap, data)
    pub(super) fn into_raw_parts(self) -> (NonNull<u8>, usize, usize, NonNull<Shared>) {
        let me = std::mem::ManuallyDrop::new(self);
        (me.ptr, me.len(), me.capacity(), me.data)
    }
}

// ===== Allocation =====

impl BytesMut {
    /// Reserves capacity for at least `additional` more bytes to be inserted.
    ///
    /// Before reallocating, `BytesMut` will attempt to reclaim any leftover capacity, either from
    /// unused allocation after calling [`advance`], or other dropped instance capacity that share
    /// memory with this `BytesMut`.
    ///
    /// `BytesMut` may reserve more space to speculatively avoid frequent reallocations.
    ///
    /// After calling reserve, capacity will be greater than or equal to `self.len() + additional`.
    /// Does nothing if capacity is already sufficient.
    ///
    /// [`advance`]: crate::bytes::Buf::advance
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        if self.capacity() - self.len() < additional {
            self.reserve_inner(additional);
        }
        unsafe { std::hint::assert_unchecked(self.capacity() - self.len() >= additional); }
    }

    /// Try to reclaim leftover capacity without allocating.
    #[inline]
    pub fn reclaim(&mut self) {
        self.reserve_inner(0);
    }

    /// Separate allocation call to allow `reserve` te be inlined
    ///
    /// Before reallocating, this will try to reclaim leftover capacity.
    ///
    /// The strategy is explain at the top of the file.
    #[cold]
    fn reserve_inner(&mut self, additional: usize) {
        let (base_raw, offset) = match shared::as_unpromoted_non_null(self.data) {
            Ok(offset) => {
                let ptr = if self.cap == 0 {
                    None // dangling pointer
                } else {
                    Some((unsafe { self.ptr.sub(offset) }, self.cap + offset))
                };
                (ptr, offset)
            },
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

        // checks if further allocation required
        if additional == 0 {
            return;
        }
        if self.cap - self.len >= additional {
            return;
        }

        let base_cap = match base_raw {
            Some((_, base_cap)) => base_cap,
            None => self.cap + offset,
        };

        let exp = base_cap.checked_mul(2);
        let add = (self.len + offset).checked_add(additional);
        let Some(new_cap) = cmp::max(exp, add).filter(|&e| e <= isize::MAX as usize) else {
            shared::capacity_overflow()
        };

        // allocation
        match base_raw {
            Some((base_ptr, base_cap)) => {
                let new_ptr = if shared::is_unpromoted(self.data.as_ptr()) {
                    shared::grow(base_ptr, base_cap, new_cap)
                } else {
                    unsafe { self.data.as_mut().grow(new_cap) }
                };
                self.ptr = unsafe { new_ptr.add(offset) };
                self.cap = new_cap - offset;
            }
            None => {
                // the buffer is not exclusive, or pointer is dangling
                //
                // `shared::grow` cannot be used, new allocation is required

                // SAFETY: `new_cap` is `1..=isize::MAX`
                let new_base_ptr = unsafe { shared::allocate(new_cap) };

                // checks for dangling pointer
                if self.cap != 0 {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            self.ptr.as_ptr().add(offset),
                            new_base_ptr.as_ptr(),
                            self.len
                        )
                    }
                }

                shared::release(self.data);
                self.ptr = new_base_ptr;
                self.cap = new_cap;
                self.data = shared::NEW_UNPROMOTED;
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
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// bytes.truncate(b"Hello".len());
    /// assert_eq!(&bytes, b"Hello");
    /// ```
    #[inline]
    pub const fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.len = len;
        }
    }

    /// Clears the `BytesMut`, removing all bytes.
    ///
    /// Note that this method has no effect on the allocated capacity.
    #[inline]
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// Removes all bytes, returning them in a new `BytesMut` instance.
    ///
    /// Returns `BytesMut` containing all bytes. After the call, the original `BytesMut` will be
    /// empty.
    ///
    /// This is an `O(1)` operation. The returned `BytesMut` share the same allocation, no copy is
    /// performed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let split = bytes.split();
    /// assert!(bytes.is_empty());
    /// assert_eq!(&split, b"Hello World!");
    /// ```
    ///
    /// Excess capacity is preserved.
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::with_capacity(16);
    /// bytes.extend_from_slice(b"Hello World!");
    /// let split = bytes.split();
    /// assert!(bytes.capacity() >= 16 - b"Hello World!".len());
    /// # drop(split);
    /// ```
    #[inline]
    pub fn split(&mut self) -> Self {
        // SAFETY: `self.len <= self.len`
        unsafe { self.split_to_unchecked(self.len) }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Returns `BytesMut` containing the bytes in the range `[0, at)`. After the call, the
    /// original `BytesMut` will be left containing the bytes `[at, len)`.
    ///
    /// This is an `O(1)` operation. The returned `BytesMut` share the same allocation, no copy is
    /// performed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let split = bytes.split_to(b"Hello ".len());
    /// assert_eq!(&split, b"Hello ");
    /// assert_eq!(&bytes, b"World!");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > self.len()`.
    #[inline]
    pub fn split_to(&mut self, at: usize) -> Self {
        match self.try_split_to(at) {
            Some(ok) => ok,
            None => split_fail(at, self.len),
        }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Returns `BytesMut` containing the bytes in the range `[0, at)`. After the call, the
    /// original `BytesMut` will be left containing the bytes `[at, len)`.
    ///
    /// Returns `None` if `at > self.len()`.
    ///
    /// This is an `O(1)` operation. The returned `BytesMut` share the same allocation, no copy is
    /// performed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let split = bytes.try_split_to(b"Hello ".len());
    /// assert_eq!(split.as_deref(), Some(&b"Hello "[..]));
    /// assert_eq!(&bytes, b"World!");
    /// ```
    #[inline]
    pub fn try_split_to(&mut self, at: usize) -> Option<Self> {
        if at <= self.len() {
            // SAFETY: `at <= self.len`
            unsafe { Some(self.split_to_unchecked(at)) }
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// `at <= self.len`
    unsafe fn split_to_unchecked(&mut self, at: usize) -> Self {
        debug_assert!(at <= self.len());
        self.increment();
        let ptr = self.ptr;
        self.ptr = unsafe { ptr.add(at) };
        self.len -= at;
        self.cap -= at;
        Self {
            ptr,
            len: at,
            cap: at,
            data: self.data,
        }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Returns `BytesMut` containing the bytes in the range `[at, len)`. After the call, the
    /// original `BytesMut` will be left containing the bytes `[0, at)`.
    ///
    /// This is an `O(1)` operation. The returned `BytesMut` share the same allocation, no copy is
    /// performed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let split_off = bytes.split_off(5);
    /// assert_eq!(&bytes, b"Hello");
    /// assert_eq!(&split_off, b" World!");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > self.len()`.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> Self {
        match self.try_split_off(at) {
            Some(ok) => ok,
            None => split_fail(at, self.len),
        }
    }

    /// Splits `BytesMut` into two at the given index.
    ///
    /// Returns `BytesMut` containing the bytes in the range `[at, len)`. After the call, the
    /// original `BytesMut` will be left containing the bytes `[0, at)`.
    ///
    /// Returns `None` if `at > self.len()`.
    ///
    /// This is an `O(1)` operation. The returned `BytesMut` share the same allocation, no copy is
    /// performed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let split_off = bytes.try_split_off(5);
    /// assert_eq!(&bytes, b"Hello");
    /// assert_eq!(split_off.as_deref(), Some(&b" World!"[..]));
    /// ```
    #[inline]
    pub fn try_split_off(&mut self, at: usize) -> Option<Self> {
        if at <= self.len() {
            // SAFETY: `at <= self.len`
            unsafe { Some(self.split_off_unchecked(at)) }
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// `at <= self.len`
    unsafe fn split_off_unchecked(&mut self, at: usize) -> Self {
        debug_assert!(at <= self.len);
        self.increment();
        let Self { len, cap, .. } = *self;
        self.len = at;
        self.cap = at;
        Self {
            ptr: unsafe { self.ptr.add(at) },
            len: len - at,
            cap: cap - at,
            data: self.data,
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
            count <= self.capacity(),
            "BytesMut::advance_unchecked out of bounds"
        );

        if let Ok(offset) = shared::as_unpromoted_non_null(self.data) {
            self.data = shared::mask_payload(self.data.as_ptr(), offset + count);
        }

        self.ptr = unsafe { self.ptr.add(count) };
        self.len -= count;
        self.cap -= count;
    }

    fn increment(&mut self) {
        match shared::as_unpromoted_non_null(self.data) {
            Ok(offset) => self.data = shared::promote_with(self.ptr, self.capacity(), offset, 2),
            Err(shared) => shared::increment(shared),
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
        debug_assert!(new_len <= self.capacity(), "length out of bounds");
        self.len = new_len;
    }

    /// Copy and append bytes to the `BytesMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello");
    /// bytes.extend_from_slice(b" World!");
    /// assert_eq!(&bytes, b"Hello World!")
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
    /// reuse allocation, and returns [`Ok`].
    ///
    /// Otherwise, it returns [`Err`] containing the same given `BytesMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let ptr = bytes.as_ptr();
    /// let mut split = bytes.split_to(6);
    ///
    /// assert_eq!(&split, b"Hello ");
    /// assert_eq!(&bytes, b"World!");
    ///
    /// assert_eq!(split.try_unsplit(bytes), Ok(()));
    /// assert_eq!(&split, &b"Hello World!"[..]);
    ///
    /// // no copy or allocation performed
    /// assert_eq!(split.as_ptr(), ptr);
    /// ```
    ///
    /// Note that the current `BytesMut` must be the one that is "in front".
    ///
    /// ```
    /// # use tcio::bytes::BytesMut;
    /// let mut bytes = BytesMut::copy_from_slice(b"Hello World!");
    /// let mut split = bytes.split_off(6);
    ///
    /// // `bytes` is the one in front in the buffer
    /// assert!(split.try_unsplit(bytes).is_err());
    /// ```
    #[inline]
    pub fn try_unsplit(&mut self, other: BytesMut) -> Result<(), BytesMut> {
        if other.capacity() == 0 {
            return Ok(());
        }

        let ptr = unsafe { self.ptr.add(self.len()) };

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

// ===== Conversion =====

impl BytesMut {
    /// Converts `self` into an immutable [`Bytes`].
    ///
    /// This is an `O(1)` operation. The returned `Bytes` reuse the same allocation, no copy is
    /// performed.
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

impl AsRef<[u8]> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
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

    fn put<T: crate::bytes::Buf>(&mut self, mut src: T)
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
        crate::bytes::Buf::advance(self, read);
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

// ===== Panics =====
// The panic code path was put into a cold function to not bloat the call site.

#[cfg_attr(not(panic = "immediate-abort"), inline(never), cold)]
#[cfg_attr(panic = "immediate-abort", inline)]
#[track_caller]
fn split_fail(at: usize, len: usize) -> ! {
    panic!("split out of bounds: at({at}) > self.len({len})")
}
