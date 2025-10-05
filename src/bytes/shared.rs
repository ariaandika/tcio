use std::{ptr::{self, NonNull}, sync::atomic::AtomicUsize};

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
    ref_count: AtomicUsize,
    ptr: NonNull<u8>,
    cap: usize,
}

impl Shared {
    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
}

// ===== Arbitrary =====

pub const fn new_unpromoted() -> *mut Shared {
    ptr::null_mut::<u8>().wrapping_add(DATA_UNPROMOTED).cast()
}

pub fn is_unpromoted(data: *const Shared) -> bool {
    data as usize & DATA_MASK == DATA_UNPROMOTED
}

pub fn is_promoted(data: *const Shared) -> bool {
    data as usize & DATA_MASK == DATA_PROMOTED
}

/// Pointer cannot be null.
///
/// To skip pointer null check, use [`to_unpromoted`].
pub fn as_unpromoted<'a>(data: *const Shared) -> Result<usize, &'a Shared> {
    if is_unpromoted(data) {
        Ok(data as usize >> RESERVED_BIT_DATA)
    } else {
        debug_assert!(!data.is_null());
        Err(unsafe { &*data })
    }
}

/// In contrast with [`as_unpromoted`], the pointer may be null because it will not be
/// dereferenced.
pub fn to_unpromoted(data: *const Shared) -> Option<usize> {
    if is_unpromoted(data) {
        Some(data as usize >> RESERVED_BIT_DATA)
    } else {
        None
    }
}

pub fn as_unpromoted_mut<'a>(data: *mut Shared) -> Result<usize, &'a mut Shared> {
    if is_unpromoted(data) {
        Ok(data as usize >> RESERVED_BIT_DATA)
    } else {
        Err(unsafe { &mut *data })
    }
}

pub fn into_unpromoted(data: *mut Shared) -> Result<usize, Box<Shared>> {
    if is_unpromoted(data) {
        Ok(data as usize >> RESERVED_BIT_DATA)
    } else {
        Err(unsafe { Box::from_raw(data) })
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
/// # Panics
///
/// The most significant bit must be unset, otherwise panics.
pub fn mask_payload(data: *mut Shared, value: usize) -> *mut Shared {
    const MSB: usize = RESERVED_BIT_DATA.rotate_right(RESERVED_BIT_DATA as _);

    assert!(is_unpromoted(data));
    assert_eq!(value & MSB, 0);

    data.with_addr((value << RESERVED_BIT_DATA) | DATA_UNPROMOTED)
}

pub fn promote_with_vec(mut vec: Vec<u8>, ref_count: usize) -> *mut Shared {
    let cap = vec.capacity();
    let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };

    // prevent heap deallocation
    let _vec = std::mem::ManuallyDrop::new(vec);

    let shared = Shared {
        ref_count: AtomicUsize::new(ref_count),
        ptr,
        cap,
    };

    Box::into_raw(Box::new(shared))
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
pub fn release(shared: Box<Shared>) {
    use std::sync::atomic::Ordering;

    // follow the drop procedure from `Arc`
    if shared.ref_count.fetch_sub(1, Ordering::Release) != 1 {
        // do not deallocate the heap
        let _shared = Box::into_raw(shared);
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
    shared.ref_count.load(Ordering::Acquire);

    unsafe {
        drop(Vec::from_raw_parts(shared.ptr.as_ptr(), 0, shared.cap));
    }
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

    if shared.ref_count.fetch_sub(1, Ordering::Release) != 1 {
        // do not deallocate the heap
        let _shared = Box::into_raw(shared);
        return None;
    }

    shared.ref_count.load(Ordering::Acquire);

    unsafe {
        Some(Vec::from_raw_parts(shared.ptr.as_ptr(), len, shared.cap))
    }
}
