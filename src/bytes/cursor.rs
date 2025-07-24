use std::slice::from_raw_parts as slice;

macro_rules! debug_invariant {
    ($me:ident) => {{
        debug_assert!($me.start <= $me.cursor, "`Cursor` invariant violated");
        debug_assert!($me.cursor <= $me.end, "`Cursor` invariant violated");
    }};
}

/// Raw bytes cursor.
///
/// Provides an API for bytes reading, with unsafe methods that skip bounds checking.
///
/// The safe API is in `peek*` and `next*` methods.
//
// INVARIANT: self.start <= self.cursor <= self.end
//
// note that even if `self.cursor == self.end`, dereferencing to slice would returns empty slice.
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
            // SAFETY: allocated objects can never be larger than `isize::MAX` bytes,
            // `self.cursor == self.end` is always safe
            end: unsafe { buf.as_ptr().add(buf.len()) },
            _p: std::marker::PhantomData,
        }
    }

    /// Returns how many [`Cursor`] has stepped forward.
    #[inline]
    pub fn steps(&self) -> usize {
        // SAFETY: invariant `self.start <= self.cursor`
        unsafe { offset_from(self.cursor, self.start) }
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub fn remaining(&self) -> usize {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { offset_from(self.end, self.cursor) }
    }

    /// Returns `true` if there is more bytes left.
    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns the original bytes.
    #[inline]
    pub fn original(&self) -> &'a [u8] {
        // SAFETY: invariant `self.start <= self.end`
        unsafe { slice(self.start, offset_from(self.end, self.start)) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { slice(self.cursor, offset_from(self.end, self.cursor)) }
    }

    // ===== Operations =====

    /// Try get the first byte without advancing cursor.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        if self.cursor == self.end {
            None
        } else {
            debug_invariant!(self);
            // SAFETY: start is still in bounds
            Some(unsafe { *self.cursor })
        }
    }

    /// Try get the first `N`-th bytes without advancing cursor.
    #[inline]
    pub fn peek_chunk<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if safe_add(self.cursor, N) <= self.end as usize {
            // SAFETY: `self.cursor` is valid until `N` bytes
            Some(unsafe { &*self.cursor.cast::<[u8; N]>() })
        } else {
            None
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
            debug_invariant!(self);
            // SAFETY: `self.cursor` is still in bounds
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
        if safe_add(self.cursor, N) <= self.end as usize {
            // SAFETY: self.cursor is valid until `N` bytes
            unsafe {
                let val = &*self.cursor.cast::<[u8; N]>();
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
        match self.find_raw(byte) {
            // SAFETY: checked by `find_raw`
            Some(len) => unsafe {
                let chunk = slice(self.cursor, len);
                self.advance(len);
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
        match self.find_raw(byte) {
            // SAFETY: checked by `find_raw`
            Some(len) => unsafe {
                let chunk = slice(self.cursor, len + 1);
                self.advance(len + 1);
                Some(chunk)
            },
            None => None,
        }
    }

    /// Returns chunk until the first found `byte`, and advance cursor past `byte`.
    ///
    /// The returned chunk will excludes the `byte`, and current cursor will not contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_split(&mut self, byte: u8) -> Option<&'a [u8]> {
        match self.find_raw(byte) {
            // SAFETY: checked by `find_raw`
            Some(len) => unsafe {
                let chunk = slice(self.cursor, len);
                self.advance(len + 1);
                Some(chunk)
            },
            None => None,
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
            safe_add(self.cursor, n) <= self.end as usize,
            "`Cursor::advance` safety violated, advancing `n` is out of bounds"
        );
        // SAFETY: asserted
        unsafe { self.cursor = self.cursor.add(n) };
        debug_invariant!(self);
    }

    /// Move cursor backwards cursor.
    ///
    /// # Safety
    ///
    /// Must not step back pass the first slice element.
    #[inline]
    pub unsafe fn step_back(&mut self, n: usize) {
        debug_assert!(
            (self.cursor as usize).checked_sub(n).unwrap() >= self.start as usize,
            "`Cursor::step_back` safety violated, stepping back `n` is out of bounds"
        );
        // SAFETY: asserted
        unsafe { self.cursor = self.cursor.sub(n) };
        debug_invariant!(self);
    }

    fn find_raw(&self, byte: u8) -> Option<usize> {
        const CHUNK_SIZE: usize = size_of::<usize>();
        const LSB: usize = usize::from_ne_bytes([1; CHUNK_SIZE]);
        const MSB: usize = usize::from_ne_bytes([128; CHUNK_SIZE]);

        debug_invariant!(self);

        let target = usize::from_ne_bytes([byte; CHUNK_SIZE]);
        let end = self.end as usize;
        let mut cursor = self.cursor;

        // INVARIANT#1: `cursor >= self.cursor`,
        // cursor only `add`-ed, and `self.cursor` is unchanged

        while safe_add(cursor, CHUNK_SIZE) < end {
            // SAFETY: by while condition,`cursor` is valid until CHUNK_SIZE bytes
            let x = usize::from_ne_bytes(unsafe { *(cursor as *const [u8; CHUNK_SIZE]) });

            // SWAR
            let xor_x = x ^ target;
            let found = xor_x.wrapping_sub(LSB) & !xor_x & MSB;

            if found != 0 {
                let pos = (found.trailing_zeros() / 8) as usize;

                // SAFETY: INVARIANT#1
                let offset = unsafe { offset_from(cursor, self.cursor) };

                // SAFETY: pointer will never exceed `isize::MAX` so `usize` would never overflow
                return Some(unsafe { pos.unchecked_add(offset) });
            }

            // SAFETY: by while condition, `cursor` is valid until CHUNK_SIZE bytes
            cursor = unsafe { cursor.add(CHUNK_SIZE) };
        }

        while cursor < self.end {
            // SAFETY: by while condition, `cursor` still in valid memory
            if unsafe { *cursor } == byte {
                // SAFETY: INVARIANT#1
                return Some(unsafe { offset_from(cursor, self.cursor) });
            } else {
                // SAFETY: Because allocated objects can never be larger than `isize::MAX` bytes,
                // `cursor == self.end` is always safe
                cursor = unsafe { cursor.add(1) };
            }
        }

        None
    }
}

/// # Safety
///
/// `end >= start`
#[inline]
unsafe fn offset_from(end: *const u8, start: *const u8) -> usize {
    // SAFETY: guaranteed by caller that `end >= start`
    unsafe { usize::try_from(end.offset_from(start)).unwrap_unchecked() }
}

/// Safely add pointer by casting to usize.
#[inline]
fn safe_add(ptr: *const u8, add: usize) -> usize {
    // SAFETY: pointer will never exceed `isize::MAX` so it would never overflow
    unsafe { (ptr as usize).unchecked_add(add) }
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
    assert_eq!(cursor.original(), BUF);

    // empty buffer
    let cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.peek().is_none());
    assert!(cursor.peek_chunk::<2>().is_none());
}

#[test]
fn test_cursor_next() {
    const BUF: [u8; 12] = *b"Content-Type";
    const BUF_LEN: usize = BUF.len();

    let mut cursor = Cursor::new(&BUF[..]);

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
    assert_eq!(cursor.original(), BUF);

    // empty buffer
    let mut cursor = Cursor::new(b"");
    assert!(!cursor.has_remaining());
    assert!(cursor.next().is_none());
    assert!(cursor.next_chunk::<2>().is_none());
    assert_eq!(cursor.as_bytes(), b"");
}

#[test]
fn test_next_find() {
    const BUF: [u8; 14] = *b"Content-Type: ";

    let mut cursor = Cursor::new(&BUF);
    assert_eq!(cursor.next_find(b'-'), Some(&b"Content"[..]));
    assert_eq!(cursor.as_bytes(), &b"-Type: "[..]);

    let mut cursor = Cursor::new(&BUF);
    assert_eq!(cursor.next_until(b'-'), Some(&b"Content-"[..]));
    assert_eq!(cursor.as_bytes(), &b"Type: "[..]);

    let mut cursor = Cursor::new(&BUF);
    assert_eq!(cursor.next_split(b'-'), Some(&b"Content"[..]));
    assert_eq!(cursor.as_bytes(), &b"Type: "[..]);

    let mut cursor = Cursor::new(&BUF);
    assert_eq!(cursor.next_find(b':'), Some(&b"Content-Type"[..]));
    assert_eq!(cursor.as_bytes(), &b": "[..]);

    let mut cursor = Cursor::new(&BUF);
    assert_eq!(cursor.next_find(b'*'), None);
    assert_eq!(cursor.as_bytes(), &BUF);
}
