#![allow(missing_docs, reason = "wip")]
use std::{
    cmp,
    mem::{ManuallyDrop, MaybeUninit},
    ptr::{self, NonNull},
    slice,
    sync::atomic::AtomicUsize,
};


// `Shared` have even number alignment, so the LSB is always unset
const _: [(); align_of::<Shared>() % 2] = [];

// const DATA_SHARED: usize = 0b0;
const DATA_OWNED: usize = 0b1;
const DATA_MASK: usize = 0b1;

const RESERVED_BIT_DATA: usize = 1;

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

struct Shared {
    count: AtomicUsize,
    vec: Vec<u8>,
}

// `data` field representation
enum Data<'a> {
    Owned { offset: usize },
    Shared(&'a Shared)
}

// `data` field mutable representation
enum DataMut<'a> {
    Owned { offset: usize },
    Shared(&'a mut Shared)
}

impl Shared {
    fn is_unique(&self) -> bool {
        use std::sync::atomic::Ordering;
        // The `Acquire` ordering synchronizes with the `Release` as
        // part of the `fetch_sub` in `Shared::release`. The `fetch_sub`
        // operation guarantees that any mutations done in other threads
        // are ordered before the `ref_count` is decremented. As such,
        // this `Acquire` will guarantee that those mutations are
        // visible to the current thread.
        self.count.load(Ordering::Acquire) == 1
    }

    // follow the clone procedure from `Arc`
    fn increment(ptr: *mut Shared) {
        use std::sync::atomic::Ordering;
        // Using a relaxed ordering is alright here, as knowledge of the
        // original reference prevents other threads from erroneously deleting
        // the object.
        //
        // As explained in the [Boost documentation][1], Increasing the
        // reference counter can always be done with memory_order_relaxed: New
        // references to an object can only be formed from an existing
        // reference, and passing an existing reference from one thread to
        // another must already provide any required synchronization.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)
        let old_size = unsafe { &*ptr }.count.fetch_add(1, Ordering::Relaxed);

        if old_size > isize::MAX as usize {
            std::process::abort();
        }
    }

    // follow the drop procedure from `Arc`
    fn release(ptr: *mut Shared) {
        use std::sync::atomic::Ordering;

        if unsafe { &*ptr }.count.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }

        // This fence is needed to prevent reordering of use of the data and
        // deletion of the data.  Because it is marked `Release`, the decreasing
        // of the reference count synchronizes with this `Acquire` fence. This
        // means that use of the data happens before decreasing the reference
        // count, which happens before this fence, which happens before the
        // deletion of the data.
        //
        // As explained in the [Boost documentation][1],
        //
        // > It is important to enforce any possible access to the object in one
        // > thread (through an existing reference) to *happen before* deleting
        // > the object in a different thread. This is achieved by a "release"
        // > operation after dropping a reference (any access to the object
        // > through this reference must obviously happened before), and an
        // > "acquire" operation before deleting the object.
        //
        // [1]: (www.boost.org/doc/libs/1_55_0/doc/html/atomic/usage_examples.html)

        // ThreadSanitizer does not support memory fences.
        unsafe { &*ptr }.count.load(Ordering::Acquire);

        drop(unsafe { Box::from_raw(ptr) });
    }
}

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
            data: DATA_OWNED as _,
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
    pub fn as_slice_mut(&mut self) -> &mut [u8] {
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
        if self.is_owned() {
            Data::Owned {
                offset: self.data as usize >> RESERVED_BIT_DATA,
            }
        } else {
            Data::Shared(unsafe { &*self.data })
        }
    }

    fn data_mut(&mut self) -> DataMut<'_> {
        if self.is_owned() {
            DataMut::Owned {
                offset: self.data as usize >> RESERVED_BIT_DATA,
            }
        } else {
            DataMut::Shared(unsafe { &mut *self.data })
        }
    }

    /// Returns `true` if the underlying vector is not yet been shared.
    ///
    /// This allows for safe full mutable operation of underlying buffer.
    fn is_owned(&self) -> bool {
        (self.data as usize & DATA_MASK) == DATA_OWNED
    }

    fn owned_offset(&self) -> usize {
        debug_assert!(self.is_owned());

        self.data as usize >> RESERVED_BIT_DATA
    }

    fn set_owned_offset(&mut self, pos: usize) {
        debug_assert!(self.is_owned());
        debug_assert!(pos <= isize::MAX as usize);

        self.data = (pos << RESERVED_BIT_DATA | DATA_OWNED) as _;
    }

    unsafe fn owned_buffer(&self) -> Vec<u8> {
        let offset = self.owned_offset();

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
        let remaining = self.cap - self.len;
        let additional = match self.data() {
            Data::Owned { offset } => offset + remaining,
            Data::Shared(shared) => shared.vec.capacity() - remaining,
        };

        self.try_reclaim(additional)
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
            DataMut::Owned { offset } => {
                let remaining = offset + (self.cap - self.len);

                // Case 1, copy the data backwards
                if remaining >= additional && offset >= len {
                    unsafe {
                        let start_ptr = ptr.sub(offset);

                        // `offset >= len` guarantee no overlap
                        ptr::copy_nonoverlapping(ptr, start_ptr, len);

                        self.ptr = NonNull::new_unchecked(start_ptr);
                        self.cap += offset;
                        self.set_owned_offset(0);

                        return true;
                    }
                }

                if !allocate {
                    return false;
                }

                unsafe {
                    let capacity = cmp::max(self.cap * 2, len + additional);
                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);

                    // drop the original buffer *after* copy
                    drop(self.owned_buffer());

                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();
                    self.set_owned_offset(0);

                    true
                }
            },
            DataMut::Shared(shared) if shared.is_unique() => {
                let shared_cap = shared.vec.capacity();
                let shared_ptr = shared.vec.as_mut_ptr();
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
                    let capacity = cmp::max(self.cap * 2, len + additional);
                    let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                    let new_ptr = new_vec.as_mut_ptr();

                    ptr::copy_nonoverlapping(ptr, new_ptr, len);

                    // release the shared buffer *after* copy
                    Shared::release(self.data);

                    self.ptr = NonNull::new_unchecked(new_ptr);
                    self.cap = new_vec.capacity();

                    true
                }
            },
            DataMut::Shared(_) if !allocate => false,
            DataMut::Shared(_) => unsafe {
                let capacity = cmp::max(self.cap * 2, len + additional);
                let mut new_vec = ManuallyDrop::new(Vec::with_capacity(capacity));
                let new_ptr = new_vec.as_mut_ptr();

                ptr::copy_nonoverlapping(ptr, new_ptr, len);

                // release the shared buffer *after* copy
                Shared::release(self.data);

                self.ptr = NonNull::new_unchecked(new_ptr);
                self.cap = new_vec.capacity();

                true
            }
        }
    }
}

impl BytesMut {
    // ===== Mutation =====

    /// Copy and append bytes to the `BytesMut`.
    #[inline]
    pub fn extend_from_slice(&mut self, extend: &[u8]) {
        let cnt = extend.len();
        self.reserve(cnt);

        unsafe {
            let dst = self.spare_capacity_mut();

            // reserved
            debug_assert!(dst.len() >= cnt);

            ptr::copy_nonoverlapping(extend.as_ptr(), dst.as_mut_ptr().cast(), cnt);

            self.len = self.len.unchecked_add(cnt);
        }
    }

    unsafe fn advance_unchecked(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        debug_assert!(
            count <= self.cap,
            "BytesMut::advance_unchecked out of bounds"
        );

        if self.is_owned() {
            self.set_owned_offset(self.owned_offset() + count);

            debug_assert!(self.owned_offset() < isize::MAX as usize);
        }

        self.ptr = unsafe { self.ptr.add(count) };
        self.len = self.len.saturating_sub(count);
        self.cap -= count;
    }

    unsafe fn switch_to_shared(&mut self, count: usize) {
        debug_assert!(self.is_owned());
        debug_assert!(count == 1 || count == 2);

        let shared = Box::new(Shared {
            vec: unsafe { self.owned_buffer() },
            count: AtomicUsize::new(count),
        });
        let shared = Box::into_raw(shared);

        self.data = shared;

        debug_assert!(!self.is_owned());
    }

    #[inline]
    unsafe fn shallow_clone(&mut self) -> BytesMut {
        unsafe {
            if self.is_owned() {
                Shared::increment(self.data);
            } else {
                self.switch_to_shared(2);
            }
            ptr::read(self)
        }
    }

    #[inline]
    pub fn split_to(&mut self, at: usize) -> BytesMut {
        assert!(
            at <= self.len(),
            "BytesMut::split_to out of bounds: {:?} <= {:?}",
            at,
            self.len(),
        );
        unsafe {
            let mut other = self.shallow_clone();
            // `at <= self.len()`
            self.advance_unchecked(at);
            other.cap = at;
            other.len = at;
            other
        }
    }

    #[inline]
    pub fn split_off(&mut self, at: usize) -> BytesMut {
        assert!(
            at <= self.capacity(),
            "BytesMut::split_off out of bounds: {:?} <= {:?}",
            at,
            self.capacity(),
        );
        unsafe {
            let mut other = self.shallow_clone();
            // `at <= self.capacity()`
            other.advance_unchecked(at);
            self.cap = at;
            self.len = cmp::min(self.len, at);
            other
        }
    }

    #[inline]
    pub fn split(&mut self) -> BytesMut {
        self.split_to(self.len())
    }

    #[inline]
    pub fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.len,
            "cannot advance past `len`: {:?} <= {:?}",
            cnt,
            self.len,
        );
        unsafe {
            // `cnt <= self.len`
            self.advance_unchecked(cnt);
        }
    }
}

impl Drop for BytesMut {
    fn drop(&mut self) {
        match self.data_mut() {
            DataMut::Owned { .. } => drop(unsafe { self.owned_buffer() }),
            DataMut::Shared(_) => Shared::release(self.data),
        }
    }
}

impl Clone for BytesMut {
    #[inline]
    fn clone(&self) -> BytesMut {
        BytesMut::from_vec(self.as_slice().to_vec())
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

