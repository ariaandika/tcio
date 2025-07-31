#![allow(missing_docs, reason = "wip")]
use std::{
    cmp,
    mem::{ManuallyDrop, MaybeUninit},
    ptr::{self, NonNull},
    slice,
};

use super::{Data, DataMut, Shared};

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

    #[inline]
    pub fn copy_from_slice(slice: &[u8]) -> BytesMut {
        BytesMut::from_vec(slice.to_vec())
    }

    /// Create new empty [`BytesMut`] with at least specified capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        BytesMut::from_vec(Vec::with_capacity(capacity))
    }

    const fn from_vec(mut vec: Vec<u8>) -> BytesMut {
        let len = vec.len();
        let cap = vec.capacity();
        let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };
        // prevent heap deallocation
        let _vec = ManuallyDrop::new(vec);
        BytesMut {
            ptr,
            len,
            cap,
            data: Shared::data_owned(),
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

    /// Clears the `BytesMut`, removing all bytes.
    #[inline]
    pub const fn clear(&mut self) {
        unsafe { self.set_len(0) };
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
            let ptr = self.ptr.as_ptr().add(self.len).cast();
            let remaining = self.cap - self.len;
            slice::from_raw_parts_mut(ptr, remaining)
        }
    }


    // inner

    fn data(&self) -> Data<'_> {
        Shared::data(self.data)
    }

    fn data_mut(&mut self) -> DataMut<'_> {
        Shared::data_mut(self.data)
    }

    /// # Safety
    ///
    /// ensure `self.data` does not have other ownership
    unsafe fn original_buffer(&self) -> Vec<u8> {
        let offset = Shared::owned_data(self.data);

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

    #[inline]
    pub fn try_reclaim(&mut self, additional: usize) -> bool {
        if self.cap - self.len >= additional {
            return true;
        }

        self.reserve_inner(additional, false)
    }

    #[inline]
    pub fn try_reclaim_full(&mut self) -> bool {
        let additional = match self.data() {
            Data::Owned { data: offset } => offset + (self.cap - self.len),
            Data::Shared(shared) => shared.capacity() - self.len,
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

        debug_assert!(self.cap - self.len < additional);

        let ptr = self.ptr.as_ptr();
        let len = self.len;

        match self.data_mut() {
            DataMut::Owned { data: offset } => {
                let remaining = offset + (self.cap - self.len);

                // Case 1, copy the data backwards
                if remaining >= additional && offset >= len {
                    unsafe {
                        let start_ptr = ptr.sub(offset);

                        // `offset >= len` guarantee no overlap
                        ptr::copy_nonoverlapping(ptr, start_ptr, len);

                        self.ptr = NonNull::new_unchecked(start_ptr);
                        self.cap += offset;

                        // reset the `offset`
                        Shared::set_owned_data(&mut self.data, 0);

                        return true;
                    }
                }

                if !allocate {
                    return false;
                }

                unsafe {
                    // follow `Vec::reserve` logic instead of `Vec::with_capacity`
                    let capacity = cmp::max(self.cap * 2, len + additional);

                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);
                    new_vec.set_len(len);

                    // drop the original buffer *after* copy
                    drop(self.original_buffer());

                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();

                    // reset the `offset`
                    Shared::set_owned_data(&mut self.data, 0);

                    true
                }
            },
            DataMut::Shared(shared) if shared.is_shared_unique() => {
                let shared_cap = shared.capacity();
                let shared_ptr = shared.as_mut_ptr();
                let offset = unsafe { ptr.offset_from(shared_ptr) } as usize;

                // reclaim the leftover tail capacity
                {
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

                        self.ptr = NonNull::new_unchecked(shared_ptr);
                        self.cap += offset;

                        return true;
                    }
                }

                if !allocate {
                    return false;
                }

                unsafe {
                    // follow `Vec::reserve` logic instead of `Vec::with_capacity`
                    let capacity = cmp::max(self.cap * 2, len + additional);

                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);
                    new_vec.set_len(len);

                    // release the shared buffer *after* copy
                    Shared::release(self.data);

                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();
                    self.data = Shared::data_owned(); // switch back to `Owned` state

                    true
                }
            },
            DataMut::Shared(_) if !allocate => false,
            DataMut::Shared(_) => unsafe {
                // follow `Vec::reserve` logic instead of `Vec::with_capacity`
                let capacity = cmp::max(self.cap * 2, len + additional);

                let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                let new_ptr = new_vec.as_mut_ptr();

                ptr::copy_nonoverlapping(ptr, new_ptr, len);
                new_vec.set_len(len);

                // release the shared buffer *after* copy
                Shared::release(self.data);

                self.ptr = NonNull::new_unchecked(new_ptr);
                self.cap = new_vec.capacity();
                self.data = Shared::data_owned(); // switch back to `Owned` state

                true
            }
        }
    }
}

impl BytesMut {
    // ===== Read =====

    #[inline]
    pub fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.len,
            "cannot advance past `len`: {:?} <= {:?}",
            cnt,
            self.len,
        );
        unsafe {
            // `cnt <= self.len`, and `self.len <= self.cap`
            self.advance_unchecked(cnt);
        }
    }

    #[inline]
    pub fn split(&mut self) -> BytesMut {
        self.split_to(self.len)
    }

    #[inline]
    pub fn split_to(&mut self, at: usize) -> BytesMut {
        assert!(
            at <= self.len,
            "BytesMut::split_to out of bounds: {:?} <= {:?}",
            at,
            self.len,
        );
        let mut clone = self.shallow_clone();
        unsafe {
            // `at <= self.len`, and `self.len <= self.cap`
            self.advance_unchecked(at);
        }
        clone.cap = at;
        clone.len = at;
        clone
    }

    #[inline]
    pub fn split_off(&mut self, at: usize) -> BytesMut {
        assert!(
            at <= self.cap,
            "BytesMut::split_off out of bounds: {:?} <= {:?}",
            at,
            self.cap,
        );
        let mut other = self.shallow_clone();
        unsafe {
            // `at <= self.cap`
            other.advance_unchecked(at);
        }
        self.cap = at;
        self.len = cmp::min(self.len, at); // could advance pass `self.len`
        other
    }

    /// # Safety
    ///
    /// `count <= self.cap`
    unsafe fn advance_unchecked(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        debug_assert!(
            count <= self.cap,
            "BytesMut::advance_unchecked out of bounds"
        );

        if let Data::Owned { data: offset } = Shared::data(self.data) {
            Shared::set_owned_data(&mut self.data, offset + count);

            debug_assert!(offset + count < isize::MAX as usize);
        }

        unsafe {
            self.ptr = self.ptr.add(count); // fn precondition
            self.len = self.len.saturating_sub(count); // could advance pass `self.len`
            self.cap = self.cap.unchecked_sub(count); // fn precondition
        }
    }

    fn shallow_clone(&mut self) -> BytesMut {
        match self.data_mut() {
            DataMut::Owned { .. } => {
                // upgrade to `Shared` repr

                // SAFETY: the ownership is transfered to `Shared`
                let vec = unsafe { self.original_buffer() };

                self.data = Box::into_raw(Box::new(Shared::from_vec(vec, 2)));

                debug_assert!(Shared::is_repr_shared(self.data));
            },
            DataMut::Shared(shared) => {
                shared.increment();
            },
        }
        // ref count incremented
        unsafe { ptr::read(self) }
    }
}

impl BytesMut {
    // ===== Write =====

    /// Copy and append bytes to the `BytesMut`.
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
}

impl Drop for BytesMut {
    fn drop(&mut self) {
        match self.data() {
            Data::Owned { .. } => {
                // SAFETY: to be drop
                unsafe { drop(self.original_buffer()) }
            },
            Data::Shared(_) => Shared::release(self.data),
        }
    }
}

impl Clone for BytesMut {
    #[inline]
    fn clone(&self) -> BytesMut {
        BytesMut::copy_from_slice(self.as_slice())
    }
}

impl Default for BytesMut {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BytesMut {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
    }
}

