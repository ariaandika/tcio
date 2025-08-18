use crate::bytes::{Bytes, BytesMut};

#[test]
fn test_cursor_buf() {
    let mut bytes = Bytes::copy_from_slice(b"Content-Type");
    let mut cursor = bytes.cursor_mut();

    cursor.advance(b"Content".len());
    cursor.advance_buf();

    assert_eq!(bytes.as_slice(), b"-Type");
}

#[test]
fn test_cursor_buf_next() {
    let mut bytes = Bytes::copy_from_slice(b"Content-Type");
    let mut cursor = bytes.cursor_mut();

    while let Some(b) = cursor.next() {
        if b == b'-' {
            break
        }
    }

    cursor.advance_buf();

    assert_eq!(bytes.as_slice(), b"Type");
}

#[test]
fn test_cursor_buf_split_to() {
    let mut bytes = Bytes::copy_from_slice(b"Content-Type");
    let mut cursor = bytes.cursor_mut();

    cursor.advance(b"Content".len());
    let split = cursor.split_to();

    assert_eq!(split.as_slice(), b"Content");
    assert_eq!(cursor.as_slice(), b"-Type");
    assert_eq!(bytes.as_slice(), b"-Type");
}

#[test]
fn test_cursor_buf_split_off() {
    let mut bytes = BytesMut::copy_from_slice(b"Content-Type");
    let mut cursor = bytes.cursor_mut();

    cursor.advance(b"Content".len());
    let split = cursor.split_off();

    assert_eq!(split.as_slice(), b"-Type");
    assert_eq!(cursor.as_slice(), b"");
    assert_eq!(bytes.as_slice(), b"Content");
}

#[test]
fn test_cursor_buf_truncate() {
    let mut bytes = BytesMut::copy_from_slice(b"Content-Type");
    let mut cursor = bytes.cursor_mut();

    cursor.advance(b"Content".len());
    cursor.truncate_buf();

    assert_eq!(cursor.as_slice(), b"");
    assert_eq!(bytes.as_slice(), b"Content");
}
