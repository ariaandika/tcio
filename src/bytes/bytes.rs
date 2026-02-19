use std::mem::{self, ManuallyDrop};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicPtr, Ordering};

use super::BytesMut;
use super::shared::{self, Shared};

/// A cheaply cloneable and sliceable chunk of contiguous memory.
pub struct Bytes {
    ptr: NonNull<u8>,
    len: usize,
    /// it is requires to be atomic,
    /// buffer promotion requires to update the ptr
    ///
    /// 1. null, static value
    /// 2. 0x__1, `data as usize >> 1` = offset from base ptr
    /// 3. 0x_00, NonNull<Shared>
    data: AtomicPtr<Shared>,
}

unsafe impl Send for Bytes {}
unsafe impl Sync for Bytes {}

impl Drop for Bytes {
    #[inline]
    fn drop(&mut self) {
        let Some(shared) = NonNull::new(*self.data.get_mut()) else {
            return;
        };
        debug_assert_ne!(self.len, 0);
        match shared::as_unpromoted(shared.as_ptr()) {
            Some(offset) => shared::deallocate(self.ptr, self.len, offset),
            None => shared::release(shared),
        }
    }
}

// ===== Constructor =====

impl Bytes {
    /// Create new empty [`Bytes`].
    #[inline]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            data: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Create new [`Bytes`] from static slice.
    ///
    /// Additionally, [`is_unique`][Bytes::is_unique] will always returns `false`.
    #[inline]
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            ptr: NonNull::new(bytes.as_ptr().cast_mut()).expect("reference cannot be null"),
            len: bytes.len(),
            data: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Create new [`Bytes`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(slice: &[u8]) -> Self {
        if slice.is_empty() {
            return Self::new_empty_with_ptr(
                NonNull::new(slice.as_ptr().cast_mut()).expect("reference cannot be null"),
            );
        }
        Self {
            ptr: shared::allocate_copy(slice),
            len: slice.len(),
            data: AtomicPtr::new(shared::NEW_UNPROMOTED.as_ptr()),
        }
    }

    /// Specialized empty `Bytes` with given pointer.
    ///
    /// This is used when split and resulting in empty `Bytes` that does not need to increment the
    /// atomic counter.
    fn new_empty_with_ptr(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            len: 0,
            data: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

impl Default for Bytes {
    #[inline]
    fn default() -> Self {
        Self::new()
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
}

// ===== Split/Slice =====

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
    /// `range` should be in bounds of bytes length, otherwise panic.
    pub fn slice(&self, range: impl core::ops::RangeBounds<usize>) -> Self {
        self.try_slice_bound(range.start_bound(), range.end_bound()).expect("out of bounds")
    }

    fn try_slice_bound(
        &self,
        start_bound: core::ops::Bound<&usize>,
        end_bound: core::ops::Bound<&usize>,
    ) -> Option<Self> {
        use core::ops::Bound;
        let begin = match start_bound {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1)?,
            Bound::Unbounded => 0,
        };
        let end = match end_bound {
            Bound::Included(&n) => n.checked_add(1)?,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self.len,
        };
        if end > self.len {
            return None;
        }
        let len = end.checked_sub(begin)?;
        // SAFETY:
        // 1. `end <= self.len`,
        // 2. `begin <= end <= self.len`
        // 3. `len <= end <= self.len`
        // 4. `self.ptr` is valid until `self.len` forward
        // 5. with `begin <= self.len`, then `self.ptr.add(begin)` is in bounds
        // 6. with `end <= self.len`, then `self.ptr.add(end) <= self.ptr.add(self.len)`
        // 7. with `len <= end`, then len` correctly represent offset from
        //    `self.ptr.add(begin)` to `self.ptr.add(end)`
        //
        // then `self.ptr.add(begin)` valid until `len` forward
        let ptr = unsafe { self.ptr.add(begin) };
        if len == 0 {
            return Some(Bytes::new_empty_with_ptr(ptr));
        }
        let mut cloned = self.clone();
        cloned.ptr = ptr;
        cloned.len = len;
        Some(cloned)
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
        #[cfg_attr(not(panic = "immediate-abort"), inline(never), cold)]
        #[cfg_attr(panic = "immediate-abort", inline)]
        #[track_caller]
        fn slice_failed() -> ! {
            panic!("slice out of bounds");
        }

        if subset.is_empty() {
            return Self::new_empty_with_ptr(self.ptr);
        }

        let self_addr = self.ptr.addr().get();
        let addr = subset.as_ptr().addr();

        // check end bounds
        if addr + subset.len() > self_addr + self.len {
            slice_failed();
        }
        // check start bounds
        let Some(offset) = addr.checked_sub(self_addr) else {
            slice_failed();
        };

        let mut cloned = self.clone();
        // SAFETY: given slice is subset of self
        cloned.ptr = unsafe { self.ptr.add(offset) };
        cloned.len = subset.len();
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
        // this introduce "tail offset",
        // which cannot be represented in unpromoted,
        // thus required to be promoted
        let data = *self.data.get_mut();
        if let Some(offset) = shared::as_unpromoted(data) {
            *self.data.get_mut() = shared::promote_with(self.ptr, self.len, offset, 1).as_ptr();
        }
        self.len = len;
    }

    /// Clears the buffer, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        *self = Self::new_empty_with_ptr(self.ptr);
    }

    pub(crate) unsafe fn advance_unchecked(&mut self, count: usize) {
        if count == self.len {
            self.clear();
            return;
        }

        debug_assert!(count <= self.len, "safety violated, out of bounds");

        let data = *self.data.get_mut();
        if let Some(offset) = shared::as_unpromoted(data) {
            // SAFETY: `data` is unpromoted
            *self.data.get_mut() = shared::mask_payload(data, offset + count).as_ptr();
        }

        // SAFETY: caller ensure `count <= self.len`, and `ptr` is valid until `self.cap` forward
        unsafe { self.ptr = self.ptr.add(count) };

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
    ///
    /// # Panics
    ///
    /// Panics if `at > self.len()`.
    #[inline]
    pub fn split_off(&mut self, at: usize) -> Self {
        match self.try_split_off(at) {
            Some(ok) => ok,
            None => panic!("split_off out of bounds: {at:?} <= {:?}", self.len()),
        }
    }

    /// Splits `Bytes` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `Bytes` contains
    /// elements `[at, capacity)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// Returns `None` if `at > self.len()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// # fn run() -> Option<()> {
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let split = bytes.try_split_off(6)?;
    /// assert_eq!(&bytes, &b"Hello "[..]);
    /// assert_eq!(&split, &b"World!"[..]);
    /// assert!(bytes.try_split_off(10).is_none());
    /// # Some(())
    /// # }
    /// # assert!(run().is_some());
    /// ```
    #[inline]
    pub fn try_split_off(&mut self, at: usize) -> Option<Self> {
        if at == 0 {
            return Some(mem::replace(self, Bytes::new_empty_with_ptr(self.ptr)));
        }
        if at == self.len {
            // SAFETY: `self.ptr.add(self.len)` is always valid
            return Some(Bytes::new_empty_with_ptr(unsafe { self.ptr.add(self.len) }));
        }
        self.split_off_inner(at)
    }

    fn split_off_inner(&mut self, at: usize) -> Option<Bytes> {
        let remain_len = self.len.checked_sub(at)?;
        let cloned = self.clone_mut(unsafe { self.ptr.add(at) }, remain_len);
        self.len = at;
        Some(cloned)
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
    ///
    /// # Panics
    ///
    /// Panics if `at > self.len()`.
    #[inline]
    pub fn split_to(&mut self, at: usize) -> Self {
        match self.try_split_to(at) {
            Some(ok) => ok,
            None => panic!("split_to out of bounds: {at:?} <= {:?}", self.len()),
        }
    }

    /// Splits `Bytes` into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `Bytes` contains
    /// elements `[0, at)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and sets a few indices.
    ///
    /// Returns `None` if `at > self.len()`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Bytes;
    /// # fn run() -> Option<()> {
    /// let mut bytes = Bytes::copy_from_slice(b"Hello World!");
    /// let split = bytes.try_split_to(6)?;
    /// assert_eq!(&bytes, &b"World!"[..]);
    /// assert_eq!(&split, &b"Hello "[..]);
    /// assert!(bytes.try_split_to(10).is_none());
    /// # Some(())
    /// # }
    /// # assert!(run().is_some());
    /// ```
    #[inline]
    pub fn try_split_to(&mut self, at: usize) -> Option<Self> {
        if at == 0 {
            return Some(Bytes::new_empty_with_ptr(self.ptr));
        }
        if at == self.len {
            let empty = Bytes::new_empty_with_ptr(unsafe { self.ptr.add(self.len) });
            return Some(mem::replace(self, empty));
        }
        match self.split_off_inner(at) {
            Some(ok) => Some(mem::replace(self, ok)),
            None => None,
        }
    }
}

// ===== Atomic Operations =====

impl Clone for Bytes {
    #[inline]
    fn clone(&self) -> Self {
        let Some(shared) = NonNull::new(self.data.load(Ordering::Relaxed)) else {
            return Self {
                ptr: self.ptr,
                len: self.len,
                data: AtomicPtr::new(std::ptr::null_mut()),
            };
        };
        match shared::as_unpromoted_non_null(shared) {
            Ok(offset) => promote_ref(self, offset, shared),
            Err(shared_ref) => {
                shared::increment(shared_ref);
                unsafe { ptr::read(self) }
            }
        }
    }
}

impl Bytes {
    /// Returns `true` if `Bytes` is the only handle in a shared buffer.
    ///
    /// `Bytes` constructed from [`Bytes::from_static`] will always returns `false`.
    #[inline]
    pub fn is_unique(&self) -> bool {
        let Some(shared) = NonNull::new(self.data.load(Ordering::Relaxed)) else {
            return false;
        };
        match shared::as_unpromoted_non_null(shared) {
            Ok(_) => true,
            Err(shared) => shared::is_unique(shared),
        }
    }

    /// Like `clone`, but because it have exclusive `&mut self`, promotion guaranteed to be
    /// exclusive and skip atomic operation.
    fn clone_mut(&mut self, ptr: NonNull<u8>, len: usize) -> Self {
        let Some(shared) = NonNull::new(self.data.load(Ordering::Relaxed)) else {
            let data = AtomicPtr::new(std::ptr::null_mut());
            return Self { ptr, len, data };
        };
        match shared::as_unpromoted_non_null(shared) {
            Ok(offset) => {
                let new_shared = shared::promote_with(self.ptr, self.len, offset, 2).as_ptr();
                // in contrast with `clone`, we have exclusive `&mut self` thus no promotion can
                // happen concurrently
                *self.data.get_mut() = new_shared;
                let data = AtomicPtr::new(new_shared);
                Self { ptr, len, data }
            }
            Err(shared_ref) => {
                shared::increment(shared_ref);
                let data = AtomicPtr::new(shared.as_ptr());
                Self { ptr, len, data }
            }
        }
    }
}

// ===== Convertion =====

impl Bytes {
    pub(super) fn into_raw_parts(self) -> (NonNull<u8>, usize, *mut Shared) {
        let mut me = ManuallyDrop::new(self);
        (me.ptr, me.len, *me.data.get_mut())
    }

    /// Converts a [`Bytes`] into a [`BytesMut`].
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    #[inline]
    pub fn into_mut(self) -> BytesMut {
        BytesMut::from(self)
    }
}

impl From<Box<[u8]>> for Bytes {
    #[inline]
    fn from(value: Box<[u8]>) -> Self {
        let len = value.len();
        let ptr = NonNull::new(Box::into_raw(value).cast()).expect("box cannot be null");
        if len == 0 {
            Self::new_empty_with_ptr(ptr)
        } else {
            let data = AtomicPtr::new(shared::NEW_UNPROMOTED.as_ptr());
            Self { ptr, len, data }
        }
    }
}

impl From<Vec<u8>> for Bytes {
    #[inline]
    fn from(vec: Vec<u8>) -> Self {
        let (ptr, len, cap) = vec.into_raw_parts();
        let ptr = NonNull::new(ptr).expect("vec cannot be null");
        if len == 0 {
            Self::new_empty_with_ptr(ptr)
        } else if len == cap {
            // this is the ideal form, `len` and `cap` can be stored in single field
            let data = AtomicPtr::new(shared::NEW_UNPROMOTED.as_ptr());
            Self { ptr, len, data }
        } else {
            // we cannot start in unpromoted for `Shared` storage
            // - we have nowhere to store capacity of the vector
            // - the `data` field already contains the offset from the start ptr
            // - if `len < cap`, there is a "tail offset", thus
            //   `len` cannot be treated as capacity
            // Current methods:
            // - `shared::promote_with_vec`: allocate `AtomicUsize`, pointer, and capacity (3 word)
            // Alternative:
            // - `into_boxed_slice`: reallocate and copy the bytes, as expensive as the vector length
            let data = AtomicPtr::new(shared::promote_with(ptr, cap, 0, 1).as_ptr());
            Self { ptr, len, data }
        }
    }
}

impl From<BytesMut> for Bytes {
    #[inline]
    fn from(value: BytesMut) -> Self {
        let (ptr, len, cap, data) = value.into_raw_parts();
        if len == 0 {
            Self::new_empty_with_ptr(ptr)
        } else if let Some(offset) = shared::as_unpromoted(data.as_ptr()) {
            // same procedure as `From<Vec<u8>>`
            if len == cap {
                let data = AtomicPtr::new(data.as_ptr());
                Self { ptr, len, data }
            } else {
                let data = AtomicPtr::new(shared::promote_with(ptr, cap, offset, 1).as_ptr());
                Self { ptr, len, data }
            }
        } else {
            // in promoted state, capacity is tracked in `Shared`
            let data = AtomicPtr::new(data.as_ptr());
            Self { ptr, len, data }
        }
    }
}

impl From<Bytes> for Box<[u8]> {
    #[inline]
    fn from(value: Bytes) -> Self {
        Vec::from(value).into_boxed_slice()
    }
}

impl From<Bytes> for Vec<u8> {
    /// Converts a [`Bytes`] into a byte vector.
    ///
    /// If [`Bytes::is_unique`] returns `true`, the buffer is consumed and returned.
    ///
    /// Otherwise, the buffer is copied to new allocation.
    fn from(value: Bytes) -> Self {
        let (ptr, len, data) = value.into_raw_parts();
        let ptr = ptr.as_ptr();
        let Some(data) = NonNull::new(data) else {
            return unsafe { slice::from_raw_parts(ptr, len) }.to_vec();
        };
        let (base_ptr, base_cap) = match shared::as_unpromoted(data.as_ptr()) {
            Some(offset) => unsafe { (ptr.sub(offset), len + offset) },
            None => match shared::release_into_raw(data) {
                Some((ptr, cap)) => (ptr.as_ptr(), cap),
                None => return unsafe { slice::from_raw_parts(ptr, len) }.to_vec(),
            },
        };
        if ptr != base_ptr {
            // `Bytes` has been `advanced`, but `Vec` cannot represent that
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

// this function marked cold because promotion in `Bytes` is rare, the common way to create `Bytes`
// is from `BytesMut` splitting and freeze, which is already promoted
//
// this function marked as inline(never) to not bloat the potentially inlined `Clone`
// implementation
#[inline(never)]
#[cold]
fn promote_ref(me: &Bytes, offset: usize, shared: NonNull<Shared>) -> Bytes {
    let new_shared = shared::promote_with(me.ptr, me.len, offset, 2);

    // because cloning is called via the `Clone` trait, which take `&self`, and `Bytes`
    // is `Sync`, cloning could happens concurrently
    match me.data.compare_exchange(
        shared.as_ptr(),
        new_shared.as_ptr(),
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(old_shared) => {
            // the returned pointer is the old pointer
            debug_assert!(std::ptr::eq(old_shared, shared.as_ptr()));
            debug_assert!(!std::ptr::eq(old_shared, new_shared.as_ptr()));

            Bytes {
                ptr: me.ptr,
                len: me.len,
                data: AtomicPtr::new(new_shared.as_ptr()),
            }
        }
        Err(promoted_shared) => {
            // concurrent promotion happens during heap allocation
            debug_assert!(!std::ptr::eq(new_shared.as_ptr(), promoted_shared));
            // the written pointer should have been promoted
            debug_assert!(shared::is_promoted(promoted_shared));

            // release the heap that failed the promotion
            shared::release(new_shared);

            // increase the shared reference
            unsafe { shared::increment(&*promoted_shared) };

            Bytes {
                ptr: me.ptr,
                len: me.len,
                data: AtomicPtr::new(promoted_shared),
            }
        }
    }
}

// ===== std traits =====

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
    fn from(value: String) { Self::from(value.into_bytes()) }
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

impl<const N: usize> PartialEq<[u8; N]> for Bytes {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_slice() == other
    }
}

impl std::io::Read for Bytes {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = buf.len().min(self.len());
        buf[..read].copy_from_slice(&self[..read]);
        super::Buf::advance(self, read);
        Ok(read)
    }
}
