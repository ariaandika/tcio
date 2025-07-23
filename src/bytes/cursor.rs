/// Pointer operations.
macro_rules! ptr {
    (len($s:expr, $e:expr)) => {{
        debug_assert!($e >= $s);
        usize::try_from($e.offset_from($s)).unwrap_unchecked()
    }};
    ($add:ident($s:expr, $e:expr)) => {
        unsafe { $s.$add($e) }
    };
    ($s:expr => $e:expr) => {
        unsafe { ptr!(len($s, $e)) }
    };
}

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
    /// Pointer to the byte after the last byte.
    end: *const u8,
    _p: std::marker::PhantomData<&'a ()>,
}

impl<'a> Cursor<'a> {
    /// Create new [`Cursor`] from an initialized buffer.
    #[inline]
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            start: buf.as_ptr(),
            cursor: buf.as_ptr(),
            end: ptr!(add(buf.as_ptr(), buf.len())),
            _p: std::marker::PhantomData,
        }
    }

    /// Returns how many [`Cursor`] has stepped forward.
    #[inline]
    pub fn steps(&self) -> usize {
        ptr!(self.start => self.cursor)
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub fn remaining(&self) -> usize {
        ptr!(self.cursor => self.end)
    }

    /// Returns `true` if there is more bytes left.
    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns the original bytes.
    #[inline]
    pub fn original(&self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.start, ptr!(len(self.start, self.end))) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        unsafe { std::slice::from_raw_parts(self.cursor, ptr!(len(self.cursor, self.end))) }
    }

    // ===== Operations =====

    /// Try get the first byte without advancing cursor.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        if self.cursor == self.end {
            None
        } else {
            debug_assert!(self.cursor < self.end);
            // SAFETY: start is still in bounds
            Some(unsafe { *self.cursor })
        }
    }

    /// Try get the first `N`-th bytes without advancing cursor.
    #[inline]
    pub fn peek_chunk<const N: usize>(&self) -> Option<&[u8; N]> {
        if ptr!(add(self.cursor, N)) > self.end {
            None
        } else {
            // SAFETY: start + N is still in bounds
            Some(unsafe { &*self.cursor.cast() })
        }
    }

    /// Try get the first byte and advance the cursor by `1`.
    #[inline]
    #[allow(
        clippy::should_implement_trait,
        reason = "specialized Iterator, see note below"
    )]
    pub fn next(&mut self) -> Option<u8> {
        // no impl Iterator, though this IS an Iterator, but all the method is optimized for bytes,
        // so callers can be mistaken to call the blanket method from Iterator trait

        if self.cursor == self.end {
            None
        } else {
            debug_assert!(self.cursor < self.end);
            // SAFETY: start is still in bounds
            unsafe {
                let val = *self.cursor;
                self.advance(1);
                Some(val)
            }
        }
    }

    /// Try get the first `N`-th bytes and advance the cursor by `N`.
    #[inline]
    pub fn next_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        if ptr!(add(self.cursor, N)) > self.end {
            None
        } else {
            // SAFETY: start + N is still in bounds
            unsafe {
                let val = &*self.cursor.cast();
                self.advance(N);
                Some(val)
            }
        }
    }

    /// Returns chunk until the first found `byte`, and advance cursor to `byte`.
    ///
    /// The returned chunk will excludes `byte`, and current cursor still contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_find(&mut self, byte: u8) -> Option<&'a [u8]> {
        match find(self.as_bytes(), byte) {
            Some(n) => unsafe {
                let chunk = std::slice::from_raw_parts(self.cursor, n);
                self.advance(n);
                Some(chunk)
            },
            None => None,
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
            // SAFETY: checked by `find`
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
            // SAFETY: checked by `find`
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
            ptr!(add(self.cursor, n)) <= self.end,
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
                        ptr!(start => value.as_ptr()) + (found.trailing_zeros() / 8) as usize,
                    );
                }

                value = rest;
            }
            None => {
                return value
                    .iter()
                    .position(|e| e == &byte)
                    .map(|pos| ptr!(start => value.as_ptr()) + pos);
            }
        }
    }
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
