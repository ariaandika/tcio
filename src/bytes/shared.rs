use std::{mem::ManuallyDrop, ptr::NonNull, sync::atomic::AtomicUsize};

// `Shared` have even number alignment, so the LSB is always unset
const _: [(); align_of::<Shared>() % 2] = [];

const DATA_SHARED: usize = 0b0;
const DATA_OWNED: usize = 0b1;
const DATA_MASK: usize = 0b1;

const RESERVED_BIT_DATA: usize = 1;

pub struct Shared {
    count: AtomicUsize,
    ptr: NonNull<u8>,
    len: usize,
    cap: usize,
}

// `data` field representation
pub enum Data<'a> {
    Owned { data: usize },
    Shared(&'a Shared)
}

// `data` field mutable representation
pub enum DataMut<'a> {
    Owned { data: usize },
    Shared(&'a mut Shared)
}

impl Shared {
    pub const fn data_owned() -> *mut Self {
        DATA_OWNED as _
    }

    /// note that this implementation immediately set `Shared` in shared mode
    pub const fn from_vec(mut vec: Vec<u8>, count: usize) -> Self {
        let len = vec.len();
        let cap = vec.capacity();
        let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };

        // prevent heap deallocation
        let _vec = ManuallyDrop::new(vec);

        Self {
            count: AtomicUsize::new(count),
            ptr,
            len,
            cap,
        }
    }

    pub const fn capacity(&self) -> usize {
        self.cap
    }

    pub const fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub fn is_repr_owned(ptr: *mut Shared) -> bool {
        (ptr as usize & DATA_MASK) == DATA_OWNED
    }

    pub fn is_repr_shared(ptr: *mut Shared) -> bool {
        (ptr as usize & DATA_MASK) == DATA_SHARED
    }

    // checks

    pub fn data<'a>(ptr: *mut Shared) -> Data<'a> {
        if Shared::is_repr_owned(ptr) {
            Data::Owned { data: Shared::owned_data(ptr) }
        } else {
            Data::Shared(unsafe { &*ptr })
        }
    }

    pub fn data_mut<'a>(ptr: *mut Shared) -> DataMut<'a> {
        if Shared::is_repr_owned(ptr) {
            DataMut::Owned { data: Shared::owned_data(ptr) }
        } else {
            DataMut::Shared(unsafe { &mut *ptr })
        }
    }

    // `Owned` repr

    pub fn owned_data(ptr: *mut Shared) -> usize {
        assert!(Self::is_repr_owned(ptr));

        ptr as usize >> RESERVED_BIT_DATA
    }

    pub fn set_owned_data(ptr: &mut *mut Shared, value: usize) {
        assert!(Self::is_repr_owned(*ptr));

        *ptr = ((value << RESERVED_BIT_DATA) | DATA_OWNED) as _;
    }

    // `Shared` state, atomic operation

    pub fn is_shared_unique(&self) -> bool {
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
    pub fn increment(&mut self) {
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
        let old_size = self.count.fetch_add(1, Ordering::Relaxed);

        if old_size > isize::MAX as usize {
            std::process::abort();
        }
    }

    // follow the drop procedure from `Arc`
    pub fn release(ptr: *mut Shared) {
        use std::sync::atomic::Ordering;

        assert!(Self::is_repr_shared(ptr));

        if unsafe { (*ptr).count.fetch_sub(1, Ordering::Release) } != 1 {
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
        unsafe { (*ptr).count.load(Ordering::Acquire) };

        unsafe {
            let me = Box::from_raw(ptr);
            drop(Vec::from_raw_parts(me.ptr.as_ptr(), me.len, me.cap));
        }
    }

    /// Release the `Shared` handle, if the reference is unique, returns the underlying buffer.
    pub fn release_into_inner(ptr: *mut Shared) -> Option<Vec<u8>> {
        use std::sync::atomic::Ordering;

        // .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Relaxed)

        if unsafe { (*ptr).count.fetch_sub(1, Ordering::Release) } != 1 {
            return None;
        }

        unsafe { (*ptr).count.load(Ordering::Acquire) };

        unsafe {
            let me = Box::from_raw(ptr);
            Some(Vec::from_raw_parts(me.ptr.as_ptr(), me.len, me.cap))
        }
    }
}

