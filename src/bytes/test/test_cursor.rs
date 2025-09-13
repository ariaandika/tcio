use crate::bytes::Cursor;

const BUF: [u8; 23] = *b"Content-Type: text/html";
const BUF_LEN: usize = BUF.len();

const BUF2: [u8; 11] = *b": text/html";
const BUF2_LEN: usize = BUF2.len();
const BUF2_ADV: usize = BUF_LEN - BUF2_LEN;
const BUF2_PREV: [u8; 12] = *b"Content-Type";

#[test]
fn test_cursor_empty() {
    let mut cursor = Cursor::new(b"");

    assert_eq!(cursor.peek(), None);
    assert_eq!(cursor.peek_nth(0), None);
    assert_eq!(cursor.peek_nth(2), None);
    assert_eq!(cursor.peek_chunk::<0>(), Some(&[]));
    assert_eq!(cursor.peek_chunk::<2>(), None);
    assert_eq!(cursor.next(), None);
    assert_eq!(cursor.next_chunk::<0>(), Some(&[]));
    assert_eq!(cursor.next_chunk::<2>(), None);
}

#[test]
fn test_cursor_peek() {
    let mut cursor = Cursor::new(&BUF[..]);

    assert_eq!(cursor.peek(), BUF.first().copied());
    assert_eq!(cursor.peek_nth(0), BUF.first().copied());
    assert_eq!(cursor.peek_nth(2), Some(BUF[2]));
    assert_eq!(cursor.peek_chunk::<0>(), Some(&[]));
    assert_eq!(cursor.peek_chunk::<2>(), BUF.first_chunk::<2>());
    assert_eq!(cursor.peek_chunk::<BUF_LEN>(), Some(&BUF));
    assert_eq!(cursor.peek_chunk::<{ BUF_LEN + 1 }>(), None);

    cursor.advance(BUF2_ADV);

    assert_eq!(cursor.peek(), BUF2.first().copied());
    assert_eq!(cursor.peek_nth(0), BUF2.first().copied());
    assert_eq!(cursor.peek_nth(2), Some(BUF2[2]));
    assert_eq!(cursor.peek_chunk::<0>(), Some(&[]));
    assert_eq!(cursor.peek_chunk::<2>(), BUF2.first_chunk::<2>());
    assert_eq!(cursor.peek_chunk::<BUF2_LEN>(), Some(&BUF2));
    assert_eq!(cursor.peek_chunk::<{ BUF2_LEN + 1 }>(), None);
}

#[test]
fn test_cursor_next() {
    let mut cursor = Cursor::new(&BUF[..]);

    assert_eq!(cursor.next_chunk::<0>(), Some(&[]));
    assert_eq!(cursor.as_slice(), &BUF[..]);

    assert_eq!(cursor.next(), BUF.first().copied());
    assert_eq!(cursor.next_chunk::<2>(), BUF[1..].first_chunk::<2>());
    assert_eq!(cursor.next_chunk::<{ BUF_LEN - 3 }>(), BUF[3..].first_chunk::<{ BUF_LEN - 3 }>());
}

#[test]
fn test_cursor_truncate() {
    let mut cursor = Cursor::new(&BUF[..]);

    assert_eq!(cursor.as_slice(), &BUF[..]);
    cursor.truncate(12);
    assert_eq!(cursor.as_slice(), &BUF[..12]);

    cursor.truncate(14);
    assert_eq!(cursor.as_slice(), &BUF[..12]);

    cursor.advance(2);
    cursor.truncate(8);
    assert_eq!(cursor.as_slice(), &BUF[2..2 + 8]);
}

#[test]
fn test_cursor_prev() {
    let mut cursor = Cursor::new(&BUF[..]);

    assert!(cursor.peek_prev().is_none());
    assert!(cursor.peek_prev_chunk::<2>().is_none());
    assert_eq!(cursor.peek_prev_chunk::<0>(), Some(&[]));

    assert_eq!(cursor.next_chunk(), Some(&BUF2_PREV));

    assert_eq!(cursor.peek_prev(), BUF2_PREV.last().copied());
    assert_eq!(cursor.peek_prev_chunk(), Some(&BUF2_PREV));
    assert_eq!(cursor.peek_prev_chunk::<0>(), Some(&[]));

    assert_eq!(cursor.prev_chunk(), Some(&BUF2_PREV));
    cursor.advance(BUF_LEN);

    assert_eq!(cursor.peek_prev_chunk(), Some(&BUF));
    assert_eq!(cursor.peek_prev(), BUF.last().copied());
}

#[test]
fn test_cursor_split_first() {
    let mut cursor = Cursor::new(&BUF[..]);

    cursor.advance(BUF2_ADV);
    assert!(cursor.has_remaining());

    let (delim, rest) = unsafe { cursor.split_first() };

    assert_eq!(delim, BUF2[0]);
    assert_eq!(rest, &BUF2[1..]);

    // last byte

    cursor.advance(BUF2_LEN - 1);
    assert_eq!(cursor.remaining(), 1);

    let (delim, rest) = unsafe { cursor.split_first() };

    assert_eq!(delim, *BUF2.last().unwrap());
    assert_eq!(rest, &[]);
}

#[test]
fn test_cursor_split_last_advanced() {
    let mut cursor = Cursor::new(&BUF[..]);

    cursor.advance(BUF2_ADV);
    assert!(cursor.steps() != 0);

    let (delim, rest) = unsafe { cursor.split_last_advanced() };

    assert_eq!(delim, *BUF2_PREV.last().unwrap());
    assert_eq!(rest, &BUF2_PREV[..BUF2_ADV - 1]);
}
