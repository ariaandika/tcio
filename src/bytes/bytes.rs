use std::{
    mem::{self, ManuallyDrop},
    ptr, slice,
    sync::atomic::{AtomicPtr, Ordering},
};

use super::{Buf, BytesMut};

/// A cheaply cloneable and sliceable chunk of contiguous memory.
pub struct Bytes {
    ptr: *const u8,
    len: usize,
    // inlined "trait object"
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

    #[inline]
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
            data: AtomicPtr::new(ptr::null_mut()),
            vtable: &STATIC_VTABLE,
        }
    }

    pub(crate) fn from_vec(mut vec: Vec<u8>) -> Bytes {
        let ptr = vec.as_mut_ptr();
        let len = vec.len();
        let cap = vec.capacity();

        // Avoid an extra allocation if possible.
        if len == cap {
            todo!()
            // return Bytes::from(vec.into_boxed_slice());
        }

        let shared = Box::into_raw(Box::new(Shared::from_vec(vec, 1)));

        Bytes {
            ptr,
            len,
            data: AtomicPtr::new(shared as _),
            vtable: &SHARED_VTABLE,
        }
    }

    pub(crate) fn from_shared(ptr: *const u8, len: usize, shared: *mut Shared) -> Bytes {
        Bytes {
            ptr,
            len,
            data: AtomicPtr::new(shared as _),
            vtable: &SHARED_VTABLE,
        }
    }

    // #[inline]
    // pub(crate) unsafe fn with_vtable(
    //     ptr: *const u8,
    //     len: usize,
    //     data: AtomicPtr<()>,
    //     vtable: &'static Vtable,
    // ) -> Bytes {
    //     Bytes {
    //         ptr,
    //         len,
    //         data,
    //         vtable,
    //     }
    // }

    /// Create new [`Bytes`] by copying given bytes.
    #[inline]
    pub fn copy_from_slice(data: &[u8]) -> Self {
        Self::from_vec(data.to_vec())
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

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.len = len;
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }

    #[inline]
    pub fn into_vec(self) -> Vec<u8> {
        let me = ManuallyDrop::new(self);
        unsafe { (me.vtable.into_vec)(&me.data, me.ptr, me.len) }
    }

    #[inline]
    pub fn try_into_mut(self) -> Result<BytesMut, Self> {
        if self.is_unique() {
            let me = ManuallyDrop::new(self);
            Ok(unsafe { (me.vtable.into_mut)(&me.data, me.ptr, me.len) })
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
            vtable: &STATIC_VTABLE,
        }
    }

    #[inline]
    const unsafe fn advance_unchecked(&mut self, count: usize) {
        debug_assert!(count <= self.len, "Bytes::advance_unchecked out of bounds");
        self.len -= count;
        self.ptr = unsafe { self.ptr.add(count) };
    }
}

impl Bytes {
    // ===== Read =====

    pub fn slice(&self, range: impl std::ops::RangeBounds<usize>) -> Self {
        use core::ops::Bound;

        let len = self.len();

        let begin = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of range"),
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1).expect("out of range"),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => len,
        };

        assert!(
            begin <= end,
            "range start must not be greater than end: {begin:?} <= {end:?}",
        );
        assert!(
            end <= len,
            "range end out of bounds: {end:?} <= {len:?}",
        );

        if end == begin {
            return Bytes::new_empty_with_ptr(self.ptr.wrapping_add(begin));
        }

        let mut ret = self.clone();

        ret.len = end - begin;
        ret.ptr = unsafe { ret.ptr.add(begin) };

        ret
    }

    pub fn slice_ref(&self, subset: &[u8]) -> Self {
        // Empty slice and empty Bytes may have their pointers reset
        // so explicitly allow empty slice to be a subslice of any slice.
        if subset.is_empty() {
            return Bytes::new();
        }

        let bytes_p = self.as_ptr() as usize;
        let bytes_len = self.len();

        let sub_p = subset.as_ptr() as usize;
        let sub_len = subset.len();

        assert!(
            sub_p >= bytes_p,
            "subset pointer ({:p}) is smaller than self pointer ({:p})",
            subset.as_ptr(),
            self.as_ptr(),
        );
        assert!(
            sub_p + sub_len <= bytes_p + bytes_len,
            "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
            self.as_ptr(),
            bytes_len,
            subset.as_ptr(),
            sub_len,
        );

        let sub_offset = sub_p - bytes_p;

        self.slice(sub_offset..(sub_offset + sub_len))
    }

    pub fn split_off(&mut self, at: usize) -> Self {
        if at == self.len() {
            return Bytes::new_empty_with_ptr(self.ptr.wrapping_add(at));
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

        self.len = at;

        unsafe { clone.advance_unchecked(at) };

        clone
    }

    pub fn split_to(&mut self, at: usize) -> Self {
        if at == self.len() {
            let end_ptr = self.ptr.wrapping_add(at);
            return mem::replace(self, Bytes::new_empty_with_ptr(end_ptr));
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

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        crate::fmt::lossy(&self.as_slice()).fmt(f)
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

    fn default() { Self::new() }
    fn deref(&self) -> &[u8] { self.as_slice() }

    fn from(value: &'static [u8]) { Bytes::from_static(value) }
    fn from(value: &'static str) { Bytes::from_static(value.as_bytes()) }
    fn from(value: Vec<u8>) { Bytes::from_vec(value) }
    fn from(value: String) { Bytes::from_vec(value.into_bytes()) }
    // fn from(value: Box<u8>) -> Bytes { todo!() }

    fn eq(&self, &other: [u8]) { <[u8]>::eq(self, other) }
    fn eq(&self, &other: &[u8]) { <[u8]>::eq(self, *other) }
    fn eq(&self, &other: str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(&self, &other: &str) { <[u8]>::eq(self, other.as_bytes()) }
    fn eq(&self, &other: Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
    fn eq(&self, &other: &Vec<u8>) { <[u8]>::eq(self, other.as_slice()) }
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
            cnt <= self.len(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.len(),
        );

        unsafe {
            self.advance_unchecked(cnt);
        }
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> super::Bytes {
        self.split_to(len)
    }
}

// ===== Vtable =====

struct Vtable {
    /// fn(data, ptr, len)
    pub clone: unsafe fn(&AtomicPtr<()>, *const u8, usize) -> Bytes,
    /// fn(data, ptr, len)
    ///
    /// `into_*` consumes the `Bytes`, returning the respective value.
    pub into_vec: unsafe fn(&AtomicPtr<()>, *const u8, usize) -> Vec<u8>,
    pub into_mut: unsafe fn(&AtomicPtr<()>, *const u8, usize) -> BytesMut,
    /// fn(data)
    pub is_unique: unsafe fn(&AtomicPtr<()>) -> bool,
    /// fn(data, ptr, len)
    pub drop: unsafe fn(&mut AtomicPtr<()>, *const u8, usize),
}

// ===== Static Vtable =====

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

unsafe fn static_into_vec(_: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Vec<u8> {
    unsafe { slice::from_raw_parts(ptr, len).to_vec() }
}

unsafe fn static_into_mut(_: &AtomicPtr<()>, ptr: *const u8, len: usize) -> BytesMut {
    unsafe { BytesMut::from_vec(slice::from_raw_parts(ptr, len).to_vec()) }
}

fn static_is_unique(_: &AtomicPtr<()>) -> bool {
    false
}

unsafe fn static_drop(_: &mut AtomicPtr<()>, _: *const u8, _: usize) {
    // nothing to drop for &'static [u8]
}

// ===== Shared Vtable =====

use super::Shared;

static SHARED_VTABLE: Vtable = Vtable {
    clone: shared_clone,
    into_vec: shared_into_vec,
    into_mut: shared_into_mut,
    is_unique: shared_is_unique,
    drop: shared_drop,
};

unsafe fn shared_clone(data: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Bytes {
    let shared = data.load(Ordering::Relaxed) as *mut Shared;
    unsafe { (*shared).increment() };
    Bytes {
        ptr,
        len,
        data: AtomicPtr::new(shared as _),
        vtable: &SHARED_VTABLE,
    }
}

unsafe fn shared_into_vec(data: &AtomicPtr<()>, ptr: *const u8, len: usize) -> Vec<u8> {
    // if its unique, take the ownership,
    // otherwise, it copies
    let shared = data.load(Ordering::Relaxed).cast();

    unsafe {
        match Shared::release_into_inner(shared) {
            Some(mut vec) => {
                ptr::copy(ptr, vec.as_mut_ptr(), len);
                vec
            }
            None => slice::from_raw_parts(ptr, len).to_vec(),
        }
    }
}

unsafe fn shared_into_mut(data: &AtomicPtr<()>, ptr: *const u8, len: usize) -> BytesMut {
    let shared = data.load(Ordering::Relaxed).cast();

    unsafe {
        match Shared::release_into_inner(shared) {
            Some(vec) => {
                let off = ptr.offset_from(vec.as_ptr()) as usize;
                let mut bytes = BytesMut::from_vec(vec);
                bytes.advance_unchecked(off);
                bytes
            },
            None => BytesMut::from_vec(slice::from_raw_parts(ptr, len).to_vec()),
        }
    }
}


pub(crate) unsafe fn shared_is_unique(data: &AtomicPtr<()>) -> bool {
    unsafe { Shared::is_shared_unique(&*data.load(Ordering::Acquire).cast()) }
}

unsafe fn shared_drop(data: &mut AtomicPtr<()>, _: *const u8, _: usize) {
    Shared::release(data.get_mut().cast());
}
