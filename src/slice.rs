//! raw bytes utilities.
use bytes::{Bytes, BytesMut};

/// Returns the pointer range of a buffer.
///
/// This is intended to be used with [`slice_of_bytes`] to keep a slice of [`BytesMut`] and
/// freezing it while keeping the slice without copying.
///
/// # Examples
///
/// ```
/// # use bytes::{BytesMut, Bytes};
/// use tcio::slice::{range_of, slice_of_bytes};
///
/// let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
/// let range = range_of(&bytesm[14..]);
///
/// // `.freeze()` require mutation on `bytesm`,
/// // so any slice reference cannot pass this point;
/// //
/// // because `.split_off()` guarantees that address
/// // does not change, its possible to store pointer as index,
/// // therefore, no lifetime restriction
/// let bytes = bytesm.split_off(12).freeze();
///
/// // Shared `Bytes`, no copy
/// let content: Bytes = slice_of_bytes(range, &bytes);
///
/// assert_eq!(content, &b"text/html"[..]);
/// ```
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
/// # Examples
///
/// ```
/// # use bytes::{BytesMut, Bytes};
/// use tcio::slice::{range_of, slice_of_bytes};
///
/// let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
/// let range = range_of(&bytesm[14..]);
///
/// // `.freeze()` require mutation on `bytesm`,
/// // so any slice reference cannot pass this point;
/// //
/// // because `.split_off()` guarantees that address
/// // does not change, its possible to store pointer as index,
/// // therefore, no lifetime restriction
/// let bytes = bytesm.split_off(12).freeze();
///
/// // Shared `Bytes`, no copy
/// let content: Bytes = slice_of_bytes(range, &bytes);
///
/// assert_eq!(content, &b"text/html"[..]);
/// ```
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

    let Some(offset) = sub_p.checked_sub(bytes_p) else {
        // assert failed: sub_p >= bytes_p
        panic!(
            "range pointer ({:p}) is smaller than `bytes` pointer ({:p})",
            sub_p as *const u8,
            bytes.as_ptr(),
        );
    };

    assert!(
        sub_p + sub_len <= bytes_p + bytes_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        bytes.as_ptr(),
        bytes_len,
        sub_p as *const u8,
        sub_len,
    );

    bytes.slice(offset..offset + sub_len)
}

/// Returns the splitted [`BytesMut`] by given pointer range.
///
/// Afterwards `bytes` contains elements `[range.end, len)`, and the returned [`BytesMut`] contains
/// elements `[range.start, range.end)`.
///
/// If the given pointer range is not exactly in the front of `bytes`, the leading bytes will be
/// dropped.
///
/// # Examples
///
/// ```
/// # use bytes::{BytesMut, Bytes};
/// use tcio::slice::{range_of, slice_of_bytes_mut};
///
/// let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
/// let range = range_of(&bytesm[14..18]);
///
/// let mut split = bytesm.split_off(12);
///
/// let content: BytesMut = slice_of_bytes_mut(range, &mut split);
///
/// // note that `: ` is dropped
/// assert_eq!(content, &b"text"[..]);
/// assert_eq!(split, &b"/html"[..]);
/// ```
///
/// # Panics
///
/// Requires that the given pointer range is in fact contained within the `bytes`, otherwise this
/// function will panic.
pub fn slice_of_bytes_mut(range: std::ops::Range<usize>, bytes: &mut BytesMut) -> BytesMut {
    let bytes_p = bytes.as_ptr() as usize;
    let bytes_len = bytes.as_ptr() as usize;
    let sub_len = range.end.saturating_sub(range.start);
    let sub_p = range.start;

    let Some(leading_len) = sub_p.checked_sub(bytes_p) else {
        // assert failed: sub_p >= bytes_p
        panic!(
            "range pointer ({:p}) is smaller than `bytes` pointer ({:p})",
            sub_p as *const u8,
            bytes.as_ptr()
        )
    };

    assert!(
        sub_p + sub_len <= bytes_p + bytes_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        bytes.as_ptr(),
        bytes_len,
        sub_p as *const u8,
        sub_len,
    );

    // `BytesMut::advance` have early returns if offset 0
    bytes::Buf::advance(bytes, leading_len);

    bytes.split_to(sub_len)
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

    let Some(offset) = sub_p.checked_sub(buf_p) else {
        // assert failed: sub_p >= bytes_p
        panic!(
            "range pointer ({:p}) is smaller than `bytes` pointer ({:p})",
            sub_p as *const u8,
            buf.as_ptr(),
        );
    };
    assert!(
        sub_p + sub_len <= buf_p + buf_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        buf.as_ptr(),
        buf_len,
        sub_p as *const u8,
        sub_len,
    );

    // SAFETY:
    // - sub_p >= buf_p
    // - sub_p + sub_len <= buf_p + buf_len
    unsafe { buf.get_unchecked(offset..offset + sub_len) }
}

// ===== Cursor =====

/// Raw bytes cursor.
///
/// Provides an API for bytes reading, with unsafe methods that skip bounds checking.
///
/// The safe API is in `peek*` and `next*` methods.
#[derive(Debug)]
pub struct Cursor<'a> {
    /// Pointer to the start of the slice
    start: *const u8,
    /// Pointer to the current cursor.
    cursor: *const u8,
    /// Pointer to the byte after the last byte, dereferencing this is UB
    end: usize,
    _p: std::marker::PhantomData<&'a ()>,
}

impl<'a> Cursor<'a> {
    /// Create new [`Cursor`] from an initialized buffer.
    #[inline]
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            start: buf.as_ptr(),
            cursor: buf.as_ptr(),
            end: buf.as_ptr() as usize + buf.len(),
            _p: std::marker::PhantomData,
        }
    }

    /// Returns how many [`Cursor`] has stepped forward.
    #[inline]
    pub fn steps(&self) -> usize {
        (self.cursor as usize) - (self.start as usize)
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.end - self.cursor as usize
    }

    /// Returns `true` if there is more bytes left.
    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns the original bytes.
    #[inline]
    pub fn original(&self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.start, self.end - self.start as usize) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.cursor, self.end - self.cursor as usize) }
    }

    // ===== Operations =====

    /// Try get the first byte without advancing cursor.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        if (self.cursor as usize) < self.end {
            // SAFETY: start is still in bounds
            Some(unsafe { *self.cursor })
        } else {
            None
        }
    }

    /// Try get the first `N`-th bytes without advancing cursor.
    #[inline]
    pub fn peek_chunk<const N: usize>(&self) -> Option<&[u8; N]> {
        if (self.cursor as usize) + N <= self.end {
            // SAFETY: start + N is still in bounds
            Some(unsafe { &*self.cursor.cast() })
        } else {
            None
        }
    }

    /// Try get the first byte and advance the cursor by `1`.
    #[inline]
    #[allow(clippy::should_implement_trait, reason = "specialized Iterator, see note below")]
    pub fn next(&mut self) -> Option<u8> {
        // no impl Iterator, though this IS an Iterator, but all the method is optimized for bytes,
        // so callers can be mistaken to call the blanket method from Iterator trait

        if (self.cursor as usize) < self.end {
            // SAFETY: start is still in bounds
            unsafe {
                let val = *self.cursor;
                self.advance(1);
                Some(val)
            }
        } else {
            None
        }
    }

    /// Try get the first `N`-th bytes and advance the cursor by `N`.
    #[inline]
    pub fn next_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        if (self.cursor as usize) + N <= self.end {
            // SAFETY: start + N is still in bounds
            unsafe {
                let val = &*self.cursor.cast();
                self.advance(N);
                Some(val)
            }
        } else {
            None
        }
    }

    /// Returns chunk until the first found `byte`, and advance cursor to `byte`.
    ///
    /// The returned chunk will excludes `byte`, and current cursor still contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_find(&mut self, byte: u8) -> Option<&'a [u8]> {
        if let Some(n) = find(self.as_bytes(), byte) {
            // SAFETY: checked by `.position()`
            unsafe {
                let chunk = std::slice::from_raw_parts(self.start, n);
                self.advance(n);
                Some(chunk)
            }
        } else {
            None
        }
    }

    /// Returns chunk to the first found `byte`, and advance cursor past `byte`.
    ///
    /// The returned chunk will include the `byte`, and current cursor will not contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_until(&mut self, byte: u8) -> Option<&'a [u8]> {
        if let Some(n) = find(self.as_bytes(), byte) {
            // SAFETY: checked by `.position()`
            unsafe {
                let chunk = std::slice::from_raw_parts(self.start, n + 1);
                self.advance(n + 1);
                Some(chunk)
            }
        } else {
            None
        }
    }

    /// Returns chunk until the first found `byte`, and advance cursor past `byte`.
    ///
    /// The returned chunk will excludes the `byte`, and current cursor will not contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_split(&mut self, byte: u8) -> Option<&'a [u8]> {
        if let Some(n) = find(self.as_bytes(), byte) {
            // SAFETY: checked by `.position()`
            unsafe {
                let chunk = std::slice::from_raw_parts(self.start, n);
                self.advance(n + 1);
                Some(chunk)
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
            (self.cursor as usize) + n <= self.end,
            "`Cursor::advance` safety violated, advancing `n` is out of bounds"
        );
        unsafe { self.cursor = self.cursor.add(n) };
    }

    /// Move cursor backwards cursor.
    ///
    /// # Safety
    ///
    /// Must not step back pass the first slice element.
    #[inline]
    pub unsafe fn step_back(&mut self, n: usize) {
        debug_assert!(
            (self.cursor as usize) - n >= self.start as usize,
            "`Cursor::step_back` safety violated, stepping back `n` is out of bounds"
        );
        unsafe { self.cursor = self.cursor.sub(n) };
    }
}

fn find(mut value: &[u8], byte: u8) -> Option<usize> {
    const CHUNK_SIZE: usize = size_of::<usize>();
    const LSB: usize = usize::from_ne_bytes([1; CHUNK_SIZE]);
    const MSB: usize = usize::from_ne_bytes([128; CHUNK_SIZE]);

    let start = value.as_ptr();

    loop {
        match value.split_first_chunk() {
            Some((&chunk, rest)) => {
                // SWAR

                let x = usize::from_ne_bytes(chunk);
                let target = usize::from_ne_bytes([byte; CHUNK_SIZE]);

                let xor_x = x ^ target;
                let found = xor_x.wrapping_sub(LSB) & !xor_x & MSB;

                if found != 0 {
                    return Some(
                        distance(start, value.as_ptr()) + (found.trailing_zeros() / 8) as usize,
                    );
                }

                value = rest;
            }
            None => {
                return value
                    .iter()
                    .position(|e| e == &byte)
                    .map(|pos| distance(start, value.as_ptr()) + pos);
            }
        }
    }
}

fn distance(start: *const u8, end: *const u8) -> usize {
    unsafe { usize::try_from(end.offset_from(start)).unwrap_unchecked() }
}

#[test]
fn test_cursor_advance() {
    const BUF: [u8; 12] = *b"Content-Type";
    const BUF_LEN: usize = BUF.len();

    let mut cursor = Cursor::new(&BUF[..]);

    assert_eq!(cursor.steps(), 0);
    assert_eq!(cursor.remaining(), BUF.len());
    assert_eq!(cursor.as_bytes(), BUF);

    assert_eq!(cursor.peek(), Some(b'C'));
    assert_eq!(cursor.peek_chunk::<0>(), Some(b""));
    assert_eq!(cursor.peek_chunk::<2>(), Some(b"Co"));
    assert_eq!(cursor.peek_chunk::<13>(), None);

    // SAFETY: checked with `first_chunk::<2>`
    unsafe { cursor.advance(2) };

    const REST: [u8; 10] = *b"ntent-Type";
    const REST_LEN: usize = REST.len();

    assert_eq!(cursor.steps(), 2);
    assert_eq!(cursor.remaining(), REST_LEN);
    assert_eq!(cursor.as_bytes(), REST);

    assert_eq!(cursor.peek(), Some(b'n'));
    assert_eq!(cursor.peek_chunk::<0>(), Some(b""));
    assert_eq!(cursor.peek_chunk::<REST_LEN>(), Some(&REST));
    assert_eq!(cursor.peek_chunk::<BUF_LEN>(), None);

    // SAFETY: checked with `first_chunk::<REST_LEN>`
    unsafe { cursor.advance(REST_LEN) };

    assert_eq!(cursor.steps(), BUF_LEN);
    assert!(!cursor.has_remaining());
    assert!(cursor.peek().is_none());
    assert!(cursor.peek_chunk::<5>().is_none());
    assert_eq!(cursor.as_bytes(), b"");

    // empty buffer
    let cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.peek().is_none());
    assert!(cursor.peek_chunk::<2>().is_none());
}

#[test]
fn test_cursor_pop_front() {
    const BUF: [u8; 12] = *b"Content-Type";
    const BUF_LEN: usize = BUF.len();

    let bytes = &BUF[..];
    let mut cursor = Cursor::new(bytes);

    assert_eq!(cursor.steps(), 0);
    assert_eq!(cursor.remaining(), BUF_LEN);
    assert_eq!(cursor.as_bytes(), BUF);

    assert_eq!(cursor.peek(), Some(b'C'));
    assert_eq!(cursor.peek_chunk::<0>(), Some(b""));
    assert_eq!(cursor.peek_chunk::<2>(), Some(b"Co"));
    assert_eq!(cursor.peek_chunk::<13>(), None);

    assert_eq!(cursor.next(), Some(b'C'));
    assert_eq!(cursor.next(), Some(b'o'));

    const REST: [u8; 10] = *b"ntent-Type";
    const REST_LEN: usize = REST.len();

    assert_eq!(cursor.steps(), 2);
    assert_eq!(cursor.remaining(), REST_LEN);
    assert_eq!(cursor.as_bytes(), REST);

    assert_eq!(cursor.peek(), Some(b'n'));
    assert_eq!(cursor.peek_chunk::<0>(), Some(b""));
    assert_eq!(cursor.peek_chunk::<REST_LEN>(), Some(&REST));
    assert_eq!(cursor.peek_chunk::<BUF_LEN>(), None);

    assert_eq!(cursor.next_chunk::<REST_LEN>(), Some(&REST));

    assert!(!cursor.has_remaining());
    assert!(cursor.peek().is_none());
    assert!(cursor.peek_chunk::<5>().is_none());
    assert_eq!(cursor.steps(), BUF_LEN);
    assert_eq!(cursor.as_bytes(), b"");

    // empty buffer
    let mut cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.next().is_none());
    assert!(cursor.next_chunk::<2>().is_none());
    assert_eq!(cursor.as_bytes(), b"");
}
