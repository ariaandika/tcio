use crate::bytes::BytesMut;

const DATA: &[u8] = b"Content-Type: text/html";

#[test]
fn test_bytes_mut() {
    let mut bufm = BytesMut::copy_from_slice(DATA);

    assert_eq!(bufm.len(), DATA.len());
    assert_eq!(bufm.as_slice(), DATA);
    assert!(bufm.spare_capacity_mut().is_empty());

    const TRUNC_LEN: usize = b"Content-Type: ".len();

    bufm.truncate(TRUNC_LEN);
    assert_eq!(bufm.spare_capacity_mut().len(), DATA.len() - TRUNC_LEN);
    assert_eq!(bufm.as_slice(), &DATA[..TRUNC_LEN]);

    bufm.extend_from_slice(b"text/html");

    assert_eq!(bufm.as_slice(), DATA);
}

#[test]
fn test_bytes_mut_promoted() {
    let mut bufm = BytesMut::copy_from_slice(DATA);
    drop(bufm.split_off(bufm.len()));

    assert_eq!(bufm.len(), DATA.len());
    assert_eq!(bufm.as_slice(), DATA);
    assert!(bufm.spare_capacity_mut().is_empty());

    const TRUNC_LEN: usize = b"Content-Type: ".len();

    bufm.truncate(TRUNC_LEN);
    assert_eq!(bufm.spare_capacity_mut().len(), DATA.len() - TRUNC_LEN);
    assert_eq!(bufm.as_slice(), &DATA[..TRUNC_LEN]);

    bufm.extend_from_slice(b"text/html");

    assert_eq!(bufm.as_slice(), DATA);
}

// Split

#[test]
fn test_bytes_mut_split_to() {
    let mut buf = BytesMut::copy_from_slice(DATA);

    let to = buf.split_to(5);

    assert_eq!(buf.as_slice(), &DATA[5..]);
    assert_eq!(to.as_slice(), &DATA[..5]);
}

#[test]
fn test_bytes_mut_split_off() {
    let mut buf = BytesMut::copy_from_slice(DATA);

    let to = buf.split_off(5);

    assert_eq!(buf.as_slice(), &DATA[..5]);
    assert_eq!(to.as_slice(), &DATA[5..]);
}

// Allocation

#[test]
fn test_bytes_mut_allocation() {
    let mut buf = BytesMut::with_capacity(128);
    let ptr = buf.as_ptr();
    let cap = buf.capacity();
    buf.extend_from_slice(DATA);

    assert_eq!(buf.spare_capacity_mut().len(), cap - DATA.len());

    buf.clear();
    assert!(buf.try_reclaim_full());

    assert_eq!(buf.spare_capacity_mut().len(), cap);
    assert_eq!(buf.as_ptr(), ptr);
}

#[test]
fn test_bytes_mut_promoted_allocation() {
    let mut buf = BytesMut::with_capacity(128);
    let ptr = buf.as_ptr();
    let cap = buf.capacity();
    buf.extend_from_slice(DATA);
    drop(buf.split_off(buf.capacity()));

    assert_eq!(buf.spare_capacity_mut().len(), cap - DATA.len());

    buf.clear();
    assert!(buf.try_reclaim_full());

    assert_eq!(buf.spare_capacity_mut().len(), cap);
    assert_eq!(buf.as_ptr(), ptr);
}

// Unsplit

#[test]
fn test_bytes_mut_unsplit() {
    let mut bytes = BytesMut::copy_from_slice(DATA);

    let other = bytes.split_off(6);

    bytes.try_unsplit(other).unwrap();
}
