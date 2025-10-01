use crate::bytes::{Bytes, BytesMut};

/// NOTE: Vector with excess capacity will create Bytes in promoted state
macro_rules! vec_excess {
    ($b:expr) => {{
        let mut vec = Vec::with_capacity($b.len() + 6);
        vec.extend($b);
        assert_ne!(vec.capacity(), vec.len());
        vec
    }};
    ($cap:expr; $($e:tt)*) => {{
        let mut vec = Vec::with_capacity($cap);
        vec.extend(&[$($e)*]);
        vec
    }};
}

macro_rules! into_vec {
    ($buf:expr, eq! = $ptr:expr, $slice:expr) => {
        let vec = $buf.into_vec();
        assert_eq!(vec.as_ptr(), $ptr);
        assert_eq!(vec.as_slice(), $slice);
    };
    ($buf:expr, ne! = $ptr:expr, $slice:expr) => {
        let vec = $buf.into_vec();
        assert_ne!(vec.as_ptr(), $ptr);
        assert_eq!(vec.as_slice(), $slice);
    };
}

macro_rules! into_mut {
    ($buf:expr, eq! = $ptr:expr, $slice:expr) => {
        let bufm = $buf.into_mut();
        assert_eq!(bufm.as_ptr(), $ptr);
        assert_eq!(bufm.as_slice(), $slice);
    };
    ($buf:expr, ne! = $ptr:expr, $slice:expr) => {
        let bufm = $buf.into_mut();
        assert_ne!(bufm.as_ptr(), $ptr);
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

// Constructor

// # Lifecycle
//
// Source
// - Vec
// - Vec Promoted
//
// Destructor
// - drop
// - into_vec
// - into_mut
//
// # Operation
//
// Offset
// - advance
// - truncate
//
// Clone
// - clone
// - clone -> drop

// Bytes(Source) -> Destructor
// Bytes(Source) -> Offset -> Destructor
// Bytes(Source) -> Clone -> Destructor
// Bytes(Source) -> Offset -> Clone -> Destructor
// Bytes(Source) -> Clone -> Offset -> Destructor
// ...

// ===== Not Mutated =====

#[test]
fn test_bytes_shared() {
    let buf = Bytes::from(vec![4; 8]);

    buf.assert_unpromoted();
    assert_eq!(buf.as_slice(), &[4; 8]);

    drop(buf);
}

#[test]
fn test_bytes_shared_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    into_vec!(buf, eq! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    into_mut!(buf, eq! = ptr, &[4; 8]);
}


#[test]
fn test_bytes_promoted() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);

    buf.assert_promoted();
    assert_eq!(buf.as_slice(), &[4; 8]);

    drop(buf);
}

#[test]
fn test_bytes_promoted_into_vec() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    into_vec!(buf, eq! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_promoted_into_mut() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    into_mut!(buf, eq! = ptr, &[4; 8]);
}

// ===== Advance =====

#[test]
fn test_bytes_shared_advanced() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());

    buf.advance(2);
    buf.assert_unpromoted();
    assert_eq!(buf.as_slice(), b"ntent-Type");

    drop(buf);
}

#[test]
fn test_bytes_shared_advanced_into_vec() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let ptr = buf.as_ptr();
    buf.advance(2);
    // note that even though we `advance`, the pointer is unchanged,
    // because Bytes prefer backward copy in favor of reallocating
    into_vec!(buf, eq! = ptr, b"ntent-Type");
}

#[test]
fn test_bytes_shared_advanced_into_mut() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let ptr = buf.as_ptr();
    buf.advance(2);
    // in contrast with `into_vec`, BytesMut can represent `advance`,
    // thus no copy or reallocating
    into_mut!(buf, eq! = ptr.wrapping_add(2), b"ntent-Type");
}


#[test]
fn test_bytes_promoted_advanced() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);

    buf.advance(2);
    buf.assert_promoted();
    assert_eq!(buf.as_slice(), b"ntent-Type");

    drop(buf);
}

#[test]
fn test_bytes_promoted_advanced_into_vec() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);
    let ptr = buf.as_ptr();
    buf.advance(2);
    into_vec!(buf, eq! = ptr, b"ntent-Type");
}

#[test]
fn test_bytes_promoted_advanced_into_mut() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);
    let ptr = buf.as_ptr();
    buf.advance(2);
    into_mut!(buf, eq! = ptr.wrapping_add(2), b"ntent-Type");
}

// ===== Truncate =====

#[test]
fn test_bytes_shared_truncate() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());

    buf.assert_unpromoted();
    buf.truncate(7);
    // unpromoted cannot handle tail offset,
    // thus it is required to be promoted
    buf.assert_promoted();

    assert_eq!(buf.as_slice(), b"Content");
    drop(buf);
}

#[test]
fn test_bytes_shared_truncate_off() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    buf.truncate_off(5);
    assert_eq!(buf.as_slice(), b"Content");
    drop(buf);
}

#[test]
fn test_bytes_shared_truncate_into_vec() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let ptr = buf.as_ptr();
    buf.truncate(7);
    into_vec!(buf, eq! = ptr, b"Content");
}

#[test]
fn test_bytes_shared_truncate_into_mut() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let ptr = buf.as_ptr();
    buf.truncate(7);
    into_vec!(buf, eq! = ptr, b"Content");
}


#[test]
fn test_bytes_promoted_truncate() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);

    buf.assert_promoted();
    buf.truncate(7);
    buf.assert_promoted();

    assert_eq!(buf.as_slice(), b"Content");
    drop(buf);
}

#[test]
fn test_bytes_promoted_truncate_off() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);
    buf.truncate_off(5);
    assert_eq!(buf.as_slice(), b"Content");
    drop(buf);
}

#[test]
fn test_bytes_promoted_truncate_into_vec() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);
    let ptr = buf.as_ptr();
    buf.truncate(7);
    into_vec!(buf, eq! = ptr, b"Content");
}

#[test]
fn test_bytes_promoted_truncate_into_mut() {
    let mut buf = Bytes::from(vec_excess![b"Content-Type"]);
    let ptr = buf.as_ptr();
    buf.truncate(7);
    into_vec!(buf, eq! = ptr, b"Content");
}

// ===== Cloned =====

#[test]
fn test_bytes_shared_cloned() {
    let buf = Bytes::from(vec![4; 8]);

    buf.assert_unpromoted();
    let cloned = buf.clone();
    buf.assert_promoted();
    cloned.assert_promoted();
    assert_eq!(cloned.as_slice(), &[4; 8]);

    drop(buf);
}

#[test]
fn test_bytes_shared_cloned_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    let _cloned = buf.clone();
    // not unique, allocate and copy required
    into_vec!(buf, ne! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_cloned_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    let _cloned = buf.clone();
    into_mut!(buf, ne! = ptr, &[4; 8]);
}


#[test]
fn test_bytes_promoted_cloned() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let _cloned = buf.clone();
    drop(buf);
}

#[test]
fn test_bytes_promoted_cloned_into_vec() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    let _cloned = buf.clone();
    into_vec!(buf, ne! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_promoted_cloned_into_mut() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    let _cloned = buf.clone();
    into_mut!(buf, ne! = ptr, &[4; 8]);
}


#[test]
fn test_bytes_shared_cloned_drop() {
    let buf = Bytes::from(vec![4; 8]);
    drop(buf.clone());
    buf.assert_promoted();
    drop(buf);
}

#[test]
fn test_bytes_shared_cloned_drop_into_vec() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    drop(buf.clone());
    // buffer is unique, no reallocation
    into_vec!(buf, eq! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_shared_cloned_drop_into_mut() {
    let buf = Bytes::from(vec![4; 8]);
    let ptr = buf.as_ptr();
    drop(buf.clone());
    into_mut!(buf, eq! = ptr, &[4; 8]);
}


#[test]
fn test_bytes_promoted_cloned_drop() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    drop(buf.clone());
    buf.assert_promoted();
    drop(buf);
}

#[test]
fn test_bytes_promoted_cloned_drop_into_vec() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    drop(buf.clone());
    into_vec!(buf, eq! = ptr, &[4; 8]);
}

#[test]
fn test_bytes_promoted_cloned_drop_into_mut() {
    let buf = Bytes::from(vec_excess![12; 4; 8]);
    let ptr = buf.as_ptr();
    drop(buf.clone());
    into_mut!(buf, eq! = ptr, &[4; 8]);
}

// ===== Split =====

#[test]
fn test_bytes_shared_split_to() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());

    buf.assert_unpromoted();
    let to = buf.split_to(7);
    buf.assert_promoted();

    assert_eq!(to.as_slice(), b"Content");
    assert_eq!(buf.as_slice(), b"-Type");

    drop(buf);
}

#[test]
#[should_panic]
fn test_bytes_shared_split_to_oob() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let _to = buf.split_to(usize::MAX);
}

#[test]
fn test_bytes_shared_split_off() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());

    buf.assert_unpromoted();
    let to = buf.split_off(7);
    buf.assert_promoted();

    assert_eq!(buf.as_slice(), b"Content");
    assert_eq!(to.as_slice(), b"-Type");

    drop(buf);
}

#[test]
#[should_panic]
fn test_bytes_shared_split_off_oob() {
    let mut buf = Bytes::from(b"Content-Type".to_vec());
    let _to = buf.split_off(usize::MAX);
}

// ===== Slice Range =====

#[test]
fn test_bytes_shared_slice() {
    let buf = Bytes::from(b"Content-Type".to_vec());

    buf.assert_unpromoted();
    let slice = buf.slice(3..8);
    buf.assert_promoted();

    assert_eq!(slice.as_slice(), &buf[3..8]);
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_oob_len() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let _slice = buf.slice(4..buf.len() + 1);
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_oob_offset() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let _slice = buf.slice(buf.len()..buf.len() * 2);
}

// ===== Slice Raw =====

#[test]
fn test_bytes_shared_slice_ref() {
    let buf = Bytes::from(b"Content-Type".to_vec());

    let slice = buf.slice_ref(&buf[3..8]);
    buf.assert_promoted();

    assert_eq!(slice.as_slice(), &buf[3..8]);
}

#[test]
fn test_bytes_shared_slice_raw_unprovenance() {
    let boxed = Box::new(b"Local111");
    let buf = Bytes::from(b"Content-Type".to_vec());

    let data = boxed.as_ptr().with_addr(buf.as_ptr().wrapping_add(3).addr());
    let slice = buf.slice_from_raw(data, 5);
    buf.assert_promoted();

    assert_eq!(slice.as_slice(), &buf[3..3 + 5]);
}

// # OOB Cases
//
// - Bytes:     [==]
//   Range: [--]
//
// - Bytes:     [==]
//   Range:   [--]
//
// - Bytes:     [==]
//   Range:   [------]
//
// - Bytes:     [==]
//   Range:       [--]
//
// - Bytes:     [==]
//   Range:         [--]

#[test]
#[should_panic]
fn test_bytes_shared_slice_raw_ptr_oob_1() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let data = buf.as_ptr().wrapping_sub(4);
    let _slice = buf.slice_from_raw(data, 2);
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_raw_ptr_oob_2() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let data = buf.as_ptr().wrapping_sub(4);
    let _slice = buf.slice_from_raw(data, 6);
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_raw_ptr_oob_3() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let data = buf.as_ptr().wrapping_sub(4);
    let _slice = buf.slice_from_raw(data, buf.len() + 8);
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_raw_ptr_oob_4() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let data = buf.as_ptr().wrapping_add(4);
    let _slice = buf.slice_from_raw(data, buf.len());
}

#[test]
#[should_panic]
fn test_bytes_shared_slice_raw_ptr_oob_5() {
    let buf = Bytes::from(b"Content-Type".to_vec());
    let data = buf.as_ptr().wrapping_add(buf.len());
    let _slice = buf.slice_from_raw(data, buf.len() + 8);
}

// ...

// TODO:
// combine advance, truncate, destructure

// BytesMut

#[test]
fn test_bytes_from_mut() {
    let bufm = BytesMut::from(vec![4; 8]);
    let buf = Bytes::from(bufm);
    let ptr = buf.as_ptr();

    buf.assert_unpromoted();
    assert!(buf.is_unique());
    assert!(!buf.is_empty());
    assert_eq!(buf.as_slice(), &[4; 8]);

    // Promoted
    let cloned = buf.clone();

    buf.assert_promoted();
    assert!(!buf.is_unique());
    assert!(!cloned.is_unique());
    assert_eq!(cloned.as_slice(), &[4; 8]);
    drop(cloned);

    assert!(buf.is_unique());
    let vec = buf.into_vec();
    assert_eq!(vec.as_ptr(), ptr);
}

impl Bytes {
    #[cfg(test)]
    #[doc(hidden)]
    fn assert_promoted(&self) {
        let ptr = self
            .data()
            .load(std::sync::atomic::Ordering::Acquire)
            .cast();
        assert!(crate::bytes::shared::is_promoted(ptr));
        let _ = unsafe { &*ptr };
    }

    #[cfg(test)]
    #[doc(hidden)]
    fn assert_unpromoted(&self) {
        let ptr = self
            .data()
            .load(std::sync::atomic::Ordering::Acquire)
            .cast();
        assert!(crate::bytes::shared::is_unpromoted(ptr));
    }
}

