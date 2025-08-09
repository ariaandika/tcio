use crate::bytes::{Buf, Bytes, BytesMut};

/// Vector with excess capacity, will create Bytes in
/// promoted state
macro_rules! vec_excess {
    ($cap:expr; $($e:tt)*) => {{
        let mut vec = Vec::with_capacity($cap);
        vec.extend(&[$($e)*]);
        vec
    }};
}

macro_rules! into_vec {
    ($buf:expr, $ptr:expr, $slice:expr) => {
        let vec = $buf.into_vec();
        assert_eq!(vec.as_ptr(), $ptr);
        assert_eq!(vec.as_slice(), $slice);
    };
}

macro_rules! into_mut {
    ($buf:expr, $ptr:expr, $slice:expr) => {
        let bufm = $buf.into_mut();
        assert_eq!(bufm.as_ptr(), $ptr);
        assert_eq!(bufm.as_slice(), $slice);
    };
}

#[test]
fn test_bytes_static_empty() {
    let empty = Bytes::new();
    assert!(!empty.is_unique());
    assert_eq!(empty.as_slice(), &[]);

    let cloned = empty.clone();
    assert!(!empty.is_unique());
    assert!(!cloned.is_unique());
    assert_eq!(cloned.as_slice(), &[]);

    let vec = cloned.into_vec();
    assert_eq!(vec.capacity(), 0);
}

#[test]
fn test_bytes_shared_unique() {
    let buf = Bytes::from(vec![4; 8]);
    buf.assert_unpromoted();
    assert!(buf.is_unique());

    let cloned = buf.clone();
    cloned.assert_promoted();
    assert!(!cloned.is_unique());

    buf.assert_promoted();
    assert!(!buf.is_unique());

    drop(cloned);

    assert!(buf.is_unique());
}

// Source
// - Vec
// - Vec Promoted
//
// Into
// - into_vec
// - into_mut
//
// Clone
// - clone
// - clone -> drop

// Bytes(Source) -> Into
// Bytes(Source) -> advance -> Into
// Bytes(Source) -> Clone -> Into
// Bytes(Source) -> advance -> Clone -> Into
// Bytes(Source) -> Clone -> advance -> Into

#[test]
fn test_bytes_shared_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    buf.assert_unpromoted();
    assert_eq!(buf.as_slice(), &[4; 8]);

    into_vec!(buf, ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    into_mut!(buf, ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_promoted_into_vec() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();

    buf.assert_promoted();
    assert_eq!(buf.as_slice(), &[4; 8]);

    into_vec!(buf, ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_promoted_into_mut() {
    let buf = Bytes::from(vec_excess![6; 4; 8]);
    let ptr = buf.as_ptr();

    into_mut!(buf, ptr, &[4; 8]);
}

// Advance

#[test]
fn test_bytes_shared_advanced_into_vec() {
    let mut buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    buf.advance(2);
    assert_eq!(buf.as_slice(), &[4; 6]);

    let vec = buf.into_vec();
    // note that even though we `advance`, the pointer unchanged,
    // because `len < cap` thus it backward copy
    assert_eq!(vec.as_ptr(), ptr);
    assert_eq!(vec.as_slice(), &[4; 6]);
}

#[test]
fn test_bytes_shared_advanced_into_mut() {
    let mut buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_slice().as_ptr();

    buf.advance(2);

    into_mut!(buf, ptr.wrapping_add(2), &[4; 6]);
}

#[test]
fn test_bytes_shared_promoted_advanced_into_vec() {
    let mut buf = Bytes::from(vec_excess![6; 4; 8]);
    let ptr = buf.as_ptr();

    buf.assert_promoted();
    buf.advance(2);
    assert_eq!(buf.as_slice(), &[4; 6]);

    into_vec!(buf, ptr, &[4; 6]);
}

#[test]
fn test_bytes_shared_promoted_advanced_into_mut() {
    let mut buf = Bytes::from(vec_excess![6; 4; 8]);
    let ptr = buf.as_ptr();

    buf.assert_promoted();
    buf.advance(2);

    into_mut!(buf, ptr.wrapping_add(2), &[4; 6]);
}

// Cloned

#[test]
fn test_bytes_shared_cloned_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    let cloned = buf.clone();

    buf.assert_promoted();
    cloned.assert_promoted();
    assert_eq!(cloned.as_slice(), &[4; 8]);
    drop(cloned);

    into_vec!(buf, ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_cloned_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    drop(buf.clone());

    into_mut!(buf, ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_cloned_and_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    let _cloned = buf.clone();

    let vec = buf.into_vec();
    // not unique, allocate and copy required
    assert_ne!(vec.as_ptr(), ptr);
    assert_eq!(vec.as_slice(), (&[4; 8]));
}

#[test]
fn test_bytes_shared_cloned_and_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();

    let _cloned = buf.clone();

    let bufm = buf.into_mut();
    assert_ne!(bufm.as_ptr(), ptr);
    assert_eq!(bufm.as_slice(), (&[4; 8]));
}

// Truncate

#[test]
fn test_bytes_shared_truncate_into_vec() {
    let mut buf = Bytes::from(vec![4; 8]);

    buf.truncate(5);

    assert_eq!(buf.as_slice(), &[4; 5]);
}

// Split

#[test]
fn test_bytes_shared_split_to() {
    let mut buf = Bytes::from(vec![4u8; 8]);

    let to = buf.split_to(5);

    buf.assert_promoted();
    assert!(!buf.is_unique());
    assert_eq!(buf.as_slice(), &[4u8; 8 - 5]);
    assert_eq!(to.as_slice(), &[4u8; 5]);
    drop(to);

    assert!(buf.is_unique());
}

#[test]
fn test_bytes_shared_split_off() {
    let mut buf = Bytes::from(vec![4u8; 8]);

    let to = buf.split_off(5);

    buf.assert_promoted();
    assert!(!buf.is_unique());
    assert_eq!(buf.as_slice(), &[4u8; 5]);
    assert_eq!(to.as_slice(), &[4u8; 8 - 5]);
    drop(to);

    assert!(buf.is_unique());
}

// BytesMut

#[test]
fn test_bytes_from_mut() {
    let bufm = BytesMut::from(vec![1, 2, 3]);
    let buf = Bytes::from(bufm);
    let ptr = buf.as_ptr();

    assert!(!buf.is_empty());
    assert!(buf.is_unique());
    assert_eq!(buf.as_slice(), &[1, 2, 3]);

    // Promoted
    let cloned = buf.clone();

    buf.assert_promoted();
    assert!(!buf.is_unique());
    assert!(!cloned.is_unique());
    assert_eq!(cloned.as_slice(), &[1, 2, 3]);
    drop(cloned);

    assert!(buf.is_unique());
    let vec = buf.into_vec();
    assert_eq!(vec.as_ptr(), ptr);
}

impl Bytes {
    #[cfg(test)]
    #[doc(hidden)]
    pub(super) fn assert_promoted(&self) {
        let ptr = self
            .data()
            .load(std::sync::atomic::Ordering::Acquire)
            .cast();
        assert!(crate::bytes::shared::is_promoted(ptr));
        let _ = unsafe { &*ptr };
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub(super) fn assert_unpromoted(&self) {
        let ptr = self
            .data()
            .load(std::sync::atomic::Ordering::Acquire)
            .cast();
        assert!(crate::bytes::shared::is_unpromoted(ptr));
    }
}

