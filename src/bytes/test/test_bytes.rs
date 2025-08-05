use crate::bytes::{Buf, Bytes, BytesMut};

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
fn test_bytes_shared() {
    let buf = Bytes::from(vec![1, 2, 3]);
    let ptr = buf.as_ptr();

    assert!(buf.is_unique());
    assert_eq!(buf.as_slice(), &[1, 2, 3]);

    let vec = buf.into_vec();
    assert_eq!(vec.as_ptr(), ptr);
}

#[test]
fn test_bytes_shared_advanced() {
    let mut buf = Bytes::from(vec![1, 2, 3]);
    let ptr = buf.as_ptr();

    assert!(buf.is_unique());

    buf.advance(2);

    assert_eq!(buf.as_slice(), &[3]);

    let vec = buf.into_vec();
    assert_eq!(vec.as_slice(), &[3]);
    // note that even though we `advance`,
    // the pointer unchanged, because the
    // behavior is backward copy
    assert_eq!(vec.as_ptr(), ptr);
}

#[test]
fn test_bytes_shared_advanced_mut() {
    let mut buf = Bytes::from(vec![1, 2, 3]);
    let ptr = buf.as_slice().as_ptr();

    assert!(buf.is_unique());

    buf.advance(2);

    assert_eq!(buf.as_slice(), &[3]);

    let bufm = buf.try_into_mut().unwrap();
    assert_eq!(bufm.as_slice(), &[3]);
    assert_eq!(bufm.as_ptr().wrapping_sub(2), ptr);
}

#[test]
fn test_bytes_shared_promoted() {
    let buf = Bytes::from(vec![1, 2, 3]);
    let ptr = buf.as_ptr();

    assert!(buf.is_unique());
    assert_eq!(buf.as_slice(), &[1, 2, 3]);

    // Promoted
    let cloned = buf.clone();

    buf.assert_promoted();
    assert!(!buf.is_unique());
    assert!(!cloned.is_unique());
    assert_eq!(cloned.as_slice(), &[1, 2, 3]);
    drop(cloned);

    let vec = buf.into_vec();
    assert_eq!(vec.as_ptr(), ptr);
}

#[test]
fn test_bytes_shared_mapped_promoted() {
    let mut vec = Vec::with_capacity(6);
    vec.extend([1, 2, 3]);
    // Already promoted
    let buf = Bytes::from(vec);
    let ptr = buf.as_ptr();

    buf.assert_promoted();
    assert!(buf.is_unique());
    assert_eq!(buf.as_slice(), &[1, 2, 3]);

    let cloned = buf.clone();

    assert!(!buf.is_unique());
    assert!(!cloned.is_unique());
    assert_eq!(cloned.as_slice(), &[1, 2, 3]);
    drop(cloned);

    assert!(buf.is_unique());
    let vec = buf.into_vec();
    assert_eq!(vec.as_ptr(), ptr);
}

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

