//! raw bytes utilities.
use bytes::Bytes;

macro_rules! example_range {
    () => {
r#"# Examples

```
# use bytes::{BytesMut, Bytes};
use tcio::slice::{range_of, slice_of_bytes};

let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
let range = range_of(&bytesm[14..]);

// `.freeze()` require mutation on `bytesm`,
// so any slice reference cannot pass this point;
//
// because `.split_off()` guarantees that address
// does not change, its possible to store pointer as index,
// therefore, no lifetime restriction
let bytes = bytesm.split_off(12).freeze();

// Shared `Bytes`, no copy
let content: Bytes = slice_of_bytes(range, &bytes);

assert_eq!(content, &b"text/html"[..]);
```"#
    };
}

/// Returns the pointer range of a buffer.
///
/// This is intended to be used with [`slice_of_bytes`] to keep a slice of [`BytesMut`] and
/// freezing it while keeping the slice without copying.
///
#[doc = example_range!()]
///
/// [`BytesMut`]: bytes::BytesMut
#[inline]
pub fn range_of(buf: &[u8]) -> std::ops::Range<usize> {
    let ptr = buf.as_ptr() as usize;
    ptr..ptr + buf.len()
}

/// Returns the shared [`Bytes`] by given pointer range.
///
/// This is intented to be used with [`range_of`] to keep a slice of [`BytesMut`] and freezing it
/// while keeping the slice without copying.
///
#[doc = example_range!()]
///
/// # Panics
///
/// Requires that the given pointer range is in fact contained within the `bytes`, otherwise this
/// function will panic.
///
/// ```should_panic
/// # use bytes::{BytesMut, Bytes};
/// use tcio::slice::{range_of, slice_of_bytes};
///
/// let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
/// let range = range_of(&bytesm[14..]);
///
/// // only contains "Content-Type"
/// let bytes = bytesm.split_to(12).freeze();
///
/// // therefore it panic because pointer range is out of bounds
/// let content: Bytes = slice_of_bytes(range, &bytes);
/// ```
///
/// [`BytesMut`]: bytes::BytesMut
pub fn slice_of_bytes(range: std::ops::Range<usize>, bytes: &Bytes) -> Bytes {
    let bytes_p = bytes.as_ptr() as usize;
    let bytes_len = bytes.as_ptr() as usize;
    let sub_len = range.end.saturating_sub(range.start);
    let sub_p = range.start;

    if sub_len == 0 {
        return Bytes::new()
    }

    assert!(
        sub_p >= bytes_p,
        "range pointer ({:p}) is smaller than `bytes` pointer ({:p})",
        sub_p as *const u8,
        bytes.as_ptr(),
    );
    assert!(
        sub_p + sub_len <= bytes_p + bytes_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        bytes.as_ptr(),
        bytes_len,
        sub_p as *const u8,
        sub_len,
    );

    let offset = sub_p.saturating_sub(bytes_p);

    bytes.slice(offset..offset + sub_len)
}

/// Returns the subset value in `buf` with returned range from [`range_of`].
///
/// # Examples
///
/// ```
/// use tcio::slice::{range_of, slice_of};
///
/// let mut bytes = b"Content-Type: text/html";
/// let range = range_of(&bytes[14..]);
///
/// let content = slice_of(range, bytes);
///
/// assert_eq!(content, &b"text/html"[..]);
/// ```
pub fn slice_of(range: std::ops::Range<usize>, buf: &[u8]) -> &[u8] {
    let buf_p = buf.as_ptr() as usize;
    let buf_len = buf.as_ptr() as usize;
    let sub_p = range.start;
    let sub_len = range.end.saturating_sub(range.start);

    if sub_len == 0 {
        return &[]
    }

    assert!(
        sub_p >= buf_p,
        "range pointer ({:p}) is smaller than `buf` pointer ({:p})",
        sub_p as *const u8,
        buf.as_ptr(),
    );
    assert!(
        sub_p + sub_len <= buf_p + buf_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        buf.as_ptr(),
        buf_len,
        sub_p as *const u8,
        sub_len,
    );

    let offset = sub_p.saturating_sub(buf_p);

    // SAFETY:
    // - sub_p >= buf_p
    // - sub_p + sub_len <= buf_p + buf_len
    unsafe { buf.get_unchecked(offset..offset + sub_len) }
}

// ===== Cursor =====

/// Raw bytes cursor.
///
/// Provides an API for bytes reading, with unsafe methods that skip bounds checking.
#[derive(Debug)]
pub struct Cursor<'a> {
    // point to the first element
    start: *const u8,
    // point to one after last element,
    end: usize,
    _p: std::marker::PhantomData<&'a ()>,
}

impl<'a> Cursor<'a> {
    /// Create new [`Cursor`] from an initialized buffer.
    #[inline]
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            start: buf.as_ptr(),
            end: buf.as_ptr() as usize + buf.len(),
            _p: std::marker::PhantomData,
        }
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.end - self.start as usize
    }

    /// Returns `true` if there is more bytes left.
    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns the current bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.start, self.remaining()) }
    }

    /// Try get the first byte.
    #[inline]
    pub fn first(&self) -> Option<u8> {
        if (self.start as usize) < self.end {
            // SAFETY: start is still in bounds
            Some(unsafe { *self.start })
        } else {
            None
        }
    }

    /// Try get the first `N`-th bytes.
    #[inline]
    pub fn first_chunk<const N: usize>(&self) -> Option<&[u8; N]> {
        if (self.start as usize) + N <= self.end {
            // SAFETY: start + N is still in bounds
            Some(unsafe { &*self.start.cast() })
        } else {
            None
        }
    }

    /// Try get the first byte, and advance the cursor by `1`.
    #[inline]
    pub fn pop_front(&mut self) -> Option<u8> {
        if (self.start as usize) < self.end {
            // SAFETY: start is still in bounds
            unsafe {
                let val = *self.start;
                self.advance(1);
                Some(val)
            }
        } else {
            None
        }
    }

    /// Try get the first `N`-th bytes, and advance the cursor by `N`.
    #[inline]
    pub fn pop_chunk_front<const N: usize>(&mut self) -> Option<&[u8; N]> {
        if (self.start as usize) + N <= self.end {
            // SAFETY: start + N is still in bounds
            unsafe {
                let val = &*self.start.cast();
                self.advance(N);
                Some(val)
            }
        } else {
            None
        }
    }

    /// Advance cursor, discarding the first `n`-th bytes.
    ///
    /// # Safety
    ///
    /// Must not advance pass slice length.
    #[inline]
    pub unsafe fn advance(&mut self, n: usize) {
        debug_assert!(
            (self.start as usize) + n <= self.end,
            "`Cursor::advance` safety violated, advancing `n` is out of bounds"
        );
        unsafe { self.start = self.start.add(n) };
    }

    /// Move cursor backwards cursor.
    ///
    /// # Safety
    ///
    /// Must not step back pass the first slice element.
    #[inline]
    pub unsafe fn step_back(&mut self, n: usize) {
        unsafe { self.start = self.start.sub(n) };
    }
}

impl Iterator for Cursor<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.pop_front()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining();
        (len, Some(len))
    }
}

impl ExactSizeIterator for Cursor<'_> {
    fn len(&self) -> usize {
        self.remaining()
    }
}

#[test]
fn test_cursor_advance() {
    const BUF: [u8; 12] = *b"Content-Type";
    const BUF_LEN: usize = BUF.len();

    let mut cursor = Cursor::new(&BUF[..]);

    assert_eq!(cursor.remaining(), BUF.len());
    assert_eq!(cursor.as_bytes(), BUF);

    assert_eq!(cursor.first(), Some(b'C'));
    assert_eq!(cursor.first_chunk::<0>(), Some(b""));
    assert_eq!(cursor.first_chunk::<2>(), Some(b"Co"));
    assert_eq!(cursor.first_chunk::<13>(), None);

    // SAFETY: checked with `first_chunk::<2>`
    unsafe { cursor.advance(2) };

    const REST: [u8; 10] = *b"ntent-Type";
    const REST_LEN: usize = REST.len();

    assert_eq!(cursor.remaining(), REST_LEN);
    assert_eq!(cursor.as_bytes(), REST);

    assert_eq!(cursor.first(), Some(b'n'));
    assert_eq!(cursor.first_chunk::<0>(), Some(b""));
    assert_eq!(cursor.first_chunk::<REST_LEN>(), Some(&REST));
    assert_eq!(cursor.first_chunk::<BUF_LEN>(), None);

    // SAFETY: checked with `first_chunk::<REST_LEN>`
    unsafe { cursor.advance(REST_LEN) };

    assert!(!cursor.has_remaining());
    assert!(cursor.first().is_none());
    assert!(cursor.first_chunk::<5>().is_none());
    assert_eq!(cursor.as_bytes(), b"");

    // empty buffer
    let cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.first().is_none());
    assert!(cursor.first_chunk::<2>().is_none());
}

#[test]
fn test_cursor_pop_front() {
    const BUF: [u8; 12] = *b"Content-Type";
    const BUF_LEN: usize = BUF.len();

    let bytes = &BUF[..];
    let mut cursor = Cursor::new(bytes);

    assert_eq!(cursor.remaining(), BUF.len());
    assert_eq!(cursor.as_bytes(), BUF);

    assert_eq!(cursor.first(), Some(b'C'));
    assert_eq!(cursor.first_chunk::<0>(), Some(b""));
    assert_eq!(cursor.first_chunk::<2>(), Some(b"Co"));
    assert_eq!(cursor.first_chunk::<13>(), None);

    assert_eq!(cursor.pop_front(), Some(b'C'));
    assert_eq!(cursor.pop_front(), Some(b'o'));

    const REST: [u8; 10] = *b"ntent-Type";
    const REST_LEN: usize = REST.len();

    assert_eq!(cursor.remaining(), REST_LEN);
    assert_eq!(cursor.as_bytes(), REST);

    assert_eq!(cursor.first(), Some(b'n'));
    assert_eq!(cursor.first_chunk::<0>(), Some(b""));
    assert_eq!(cursor.first_chunk::<REST_LEN>(), Some(&REST));
    assert_eq!(cursor.first_chunk::<BUF_LEN>(), None);

    assert_eq!(cursor.pop_chunk_front::<REST_LEN>(), Some(&REST));

    assert!(!cursor.has_remaining());
    assert!(cursor.first().is_none());
    assert!(cursor.first_chunk::<5>().is_none());
    assert_eq!(cursor.as_bytes(), b"");

    // empty buffer
    let mut cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.pop_front().is_none());
    assert!(cursor.pop_chunk_front::<2>().is_none());
    assert_eq!(cursor.as_bytes(), b"");
}
