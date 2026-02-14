use core::ptr::{self, NonNull};
use core::sync::atomic::AtomicUsize;
use core::alloc::Layout;
use base_alloc::alloc;

extern crate alloc as base_alloc;

/// even number alignment means the LSB is always unset
///
/// this represent that the pointer is promoted
const _: [(); align_of::<Shared>() % 2] = [];

const DATA_PROMOTED: usize = 0b0;
const DATA_UNPROMOTED: usize = 0b1;
const DATA_MASK: usize = 0b1;

const RESERVED_BIT_DATA: usize = 1;

/// RESERVED_BIT_DATA must be `1` because of logic below
const _: [(); 1] = [(); RESERVED_BIT_DATA];

#[derive(Debug)]
pub struct Shared {
    ptr: NonNull<u8>,
    cap: usize,
    ref_count: AtomicUsize,
}

impl Shared {
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    pub const fn as_non_null(&self) -> NonNull<u8> {
        self.ptr
    }

    pub fn grow(&mut self, new_cap: usize) -> NonNull<u8> {
        self.ptr = self::grow(self.ptr, self.cap, new_cap);
        self.cap = new_cap;
        self.ptr
    }
}

// ===== Allocation =====

pub fn allocate(cap: usize) -> NonNull<u8> {
    unsafe {
        let layout = Layout::from_size_align_unchecked(cap, 1);
        match NonNull::new(alloc::alloc(layout)) {
            Some(ok) => ok,
            None => alloc::handle_alloc_error(layout)
        }
    }
}

pub fn allocate_copy(slice: &[u8]) -> NonNull<u8> {
    unsafe {
        let mem = allocate(slice.len());
        ptr::copy_nonoverlapping(slice.as_ptr(), mem.as_ptr(), slice.len());
        mem
    }
}

pub fn grow(ptr: NonNull<u8>, old_cap: usize, new_cap: usize) -> NonNull<u8> {
    unsafe {
        let layout = Layout::from_size_align_unchecked(old_cap, 1);
        match NonNull::new(alloc::realloc(ptr.as_ptr(), layout, new_cap)) {
            Some(ok) => ok,
            None => alloc::handle_alloc_error(layout)
        }
    }
}

pub fn deallocate(ptr: NonNull<u8>, cap: usize, offset: usize) {
    unsafe {
        let layout = Layout::from_size_align_unchecked(cap + offset, 1);
        alloc::dealloc(ptr.as_ptr().sub(offset), layout);
    }
}

// ===== Arbitrary =====

pub const NEW_UNPROMOTED: NonNull<Shared> =
    NonNull::new(ptr::null_mut::<u8>().wrapping_add(DATA_UNPROMOTED).cast()).expect("ptr is 1");

pub const fn new_unpromoted() -> NonNull<Shared> {
    NonNull::new(ptr::null_mut::<u8>().wrapping_add(DATA_UNPROMOTED).cast()).expect("ptr is 1")
}

pub fn is_unpromoted(data: *const Shared) -> bool {
    data.addr() & DATA_MASK == DATA_UNPROMOTED
}

pub fn is_promoted(data: *const Shared) -> bool {
    data.addr() & DATA_MASK == DATA_PROMOTED
}

pub fn as_unpromoted_non_null<'a>(data: NonNull<Shared>) -> Result<usize, &'a Shared> {
    if is_unpromoted(data.as_ptr()) {
        Ok(data.as_ptr().addr() >> RESERVED_BIT_DATA)
    } else {
        Err(unsafe { data.as_ref() })
    }
}

pub fn as_unpromoted(data: *const Shared) -> Option<usize> {
    if is_unpromoted(data) {
        Some(data.addr() >> RESERVED_BIT_DATA)
    } else {
        None
    }
}

pub fn into_unpromoted(data: NonNull<Shared>) -> Result<usize, Box<Shared>> {
    if is_unpromoted(data.as_ptr()) {
        Ok(data.as_ptr().addr() >> RESERVED_BIT_DATA)
    } else {
        Err(unsafe { Box::from_raw(data.as_ptr()) })
    }
}

// ===== Unpromoted =====

/// Mask the arbitrary payload with `usize`.
///
/// `Shared` requires that the least significant bit is unset to denote unpromoted buffer.
///
/// For convenience, this function mask the value such that the requirements is the most
/// significant bit is unset.
///
/// In other word, `0 <= value <= isize::MAX`.
///
/// # Safety
///
/// `data` must be unpromoted.
///
/// `value` most significant bit must be unset.
pub unsafe fn mask_payload(data: *mut Shared, value: usize) -> NonNull<Shared> {
    const MSB: usize = RESERVED_BIT_DATA.rotate_right(RESERVED_BIT_DATA as _);

    debug_assert!(is_unpromoted(data));
    debug_assert_eq!(value & MSB, 0);

    // SAFETY: `| DATA_UNPROMOTED` will made the pointer nonnull
    unsafe {
        NonNull::new_unchecked(data.with_addr((value << RESERVED_BIT_DATA) | DATA_UNPROMOTED))
    }
}

pub fn build_vec(ptr: NonNull<u8>, cap: usize, offset: usize) -> Vec<u8> {
    unsafe { Vec::from_raw_parts(ptr.as_ptr().sub(offset), 0, cap + offset) }
}

pub fn promote_with(ptr: NonNull<u8>, cap: usize, offset: usize, ref_count: usize) -> NonNull<Shared> {
    NonNull::new(Box::into_raw(Box::new(Shared {
        ref_count: AtomicUsize::new(ref_count),
        ptr: unsafe { ptr.sub(offset) },
        cap: cap + offset,
    })))
    .expect("box cannot be null")
}

// ===== Promoted =====

pub fn is_unique(shared: &Shared) -> bool {
    use std::sync::atomic::Ordering;
    // The `Acquire` ordering synchronizes with the `Release` as
    // part of the `fetch_sub` in `Shared::release`. The `fetch_sub`
    // operation guarantees that any mutations done in other threads
    // are ordered before the `ref_count` is decremented. As such,
    // this `Acquire` will guarantee that those mutations are
    // visible to the current thread.
    shared.ref_count.load(Ordering::Acquire) == 1
}

// follow the clone procedure from `Arc`
pub fn increment(shared: &Shared) {
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
    let old_size = shared.ref_count.fetch_add(1, Ordering::Relaxed);

    if old_size > isize::MAX as usize {
        std::process::abort();
    }
}

#[allow(clippy::boxed_local, reason = "`Shared` always in the heap")]
pub fn release(shared: NonNull<Shared>) {
    debug_assert!(self::is_promoted(shared.as_ptr()));
    // SAFETY: `release_into_vec` with `0` will always safe
    if let Some((ptr, cap)) = self::release_into_raw(shared) {
        deallocate(ptr, cap, 0);
    }
}

#[allow(clippy::boxed_local, reason = "`Shared` always in the heap")]
pub fn release_into_raw(shared: NonNull<Shared>) -> Option<(NonNull<u8>, usize)> {
    use std::sync::atomic::Ordering;
    debug_assert!(self::is_promoted(shared.as_ptr()));

    // follow the drop procedure from `Arc`
    if unsafe { shared.as_ref() }.ref_count.fetch_sub(1, Ordering::Release) != 1 {
        return None;
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
    std::sync::atomic::fence(Ordering::Acquire);

    // `Shared` is unique, thus gaining exclusive ownership
    let shared = unsafe { Box::from_raw(shared.as_ptr()) };
    Some((shared.ptr, shared.cap))
}

/// Release the `Shared` handle, if the reference is unique, returns the underlying buffer with
/// given length of initialized data.
///
/// # Safety
///
/// Caller must ensure that `len` of data is initialized.
#[allow(clippy::boxed_local, reason = "`Shared` always in the heap")]
pub unsafe fn release_into_vec(shared: Box<Shared>, len: usize) -> Option<Vec<u8>> {
    use std::sync::atomic::Ordering;

    // follow the drop procedure from `Arc`
    if shared.ref_count.fetch_sub(1, Ordering::Release) != 1 {
        // do not deallocate the heap
        let _shared = Box::into_raw(shared);
        return None;
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
    std::sync::atomic::fence(Ordering::Acquire);

    unsafe { Some(Vec::from_raw_parts(shared.ptr.as_ptr(), len, shared.cap)) }
}
