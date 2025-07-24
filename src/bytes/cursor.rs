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

    // ===== Reference =====

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

    // ===== Peek =====

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

    // ===== Next =====

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

    // ===== Find SIMD =====

    /// Returns chunk until the first found `byte`, and advance cursor to `byte`.
    ///
    /// The returned chunk will *excludes* `byte`, and current cursor still contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_find(&mut self, byte: u8) -> Option<&'a [u8]> {
        match self.find_inner(byte) {
            // SAFETY: checked by `find_inner`
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
    /// The returned chunk will *includes* the `byte`, and current cursor will not contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_until(&mut self, byte: u8) -> Option<&'a [u8]> {
        match self.find_inner(byte) {
            // SAFETY: checked by `find_inner`
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
    /// The returned chunk will `excludes` the `byte`, and current cursor will not contains `byte`.
    ///
    /// If `byte` is not found, returns `None`, and cursor is not advanced.
    #[inline]
    pub fn next_split(&mut self, byte: u8) -> Option<&'a [u8]> {
        match self.find_inner(byte) {
            // SAFETY: checked by `find_inner`
            Some(len) => unsafe {
                let chunk = slice(self.cursor, len);
                self.advance(len + 1);
                Some(chunk)
            },
            None => None,
        }
    }

    // ===== Advance / Step Back =====

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

    // ===== Forking =====

    /// Copy the internal state to a new [`Cursor`].
    ///
    /// This can be usefull for more complex peeking before advancing the cursor.
    ///
    /// When peeking complete, use [`Cursor::apply`] or [`Cursor::apply_to`] to apply the forked
    /// state to the parent [`Cursor`].
    #[inline(always)]
    pub fn fork(&self) -> Cursor<'a> {
        Cursor {
            start: self.start,
            cursor: self.cursor,
            end: self.end,
            _p: std::marker::PhantomData,
        }
    }

    /// Apply other [`Cursor`] state to this [`Cursor`].
    ///
    /// This is intented to be used with [`Cursor::fork`] after completed peeking.
    #[inline(always)]
    pub fn apply(&mut self, cursor: Cursor<'a>) {
        *self = cursor;
    }

    /// Apply current state to other [`Cursor`].
    ///
    /// This is intented to be used with [`Cursor::fork`] after completed peeking.
    #[inline(always)]
    pub fn apply_to(self, other: &mut Cursor<'a>) {
        *other = self;
    }

    // ===== Internal =====

    #[inline]
    fn find_inner(&self, byte: u8) -> Option<usize> {
        debug_invariant!(self);

        if self.remaining() < 8 {
            self.find_iter(self.cursor, byte)
        } else {
            self.find_swar(byte)
        }
    }

    fn find_swar(&self, byte: u8) -> Option<usize> {
        const CHUNK_SIZE: usize = size_of::<usize>();
        const LSB: usize = usize::from_ne_bytes([1; CHUNK_SIZE]);
        const MSB: usize = usize::from_ne_bytes([128; CHUNK_SIZE]);

        let target = usize::from_ne_bytes([byte; CHUNK_SIZE]);
        let end = self.end as usize;
        let mut cursor = self.cursor;

        while safe_add(cursor, CHUNK_SIZE) < end {
            // SAFETY: by while condition,`cursor` is valid until CHUNK_SIZE bytes
            let x = usize::from_ne_bytes(unsafe { *cursor.cast::<[u8; CHUNK_SIZE]>() });

            // SWAR
            // `x ^ target` all matching bytes will be 0x00
            //
            // `xor_x.wrapping_sub(LSB)` matching bytes will wrap to 0xFF
            // `xor_x!` matching bytes will be 0xFF
            //
            // bitwise AND both, resulting:
            // - matched byte to be 0xFF
            // - non-matched to be 0x00
            //
            // bitwise AND with MSB, resulting only the most
            // significant bit of the matched byte to be set
            //
            // if no match found, all bytes will be 0x00
            //
            // otherwise, `.trailing_zeros() / 8` returns
            // the first byte index that is matched

            let xor_x = x ^ target;
            let found = xor_x.wrapping_sub(LSB) & !xor_x & MSB;

            if found != 0 {
                let pos = (found.trailing_zeros() / 8) as usize;

                // SAFETY: `cursor >= self.cursor`,
                // cursor only `add`-ed, and `self.cursor` is unchanged
                let offset = unsafe { offset_from(cursor, self.cursor) };

                // SAFETY: pointer will never exceed `isize::MAX` so `usize` would never overflow
                return Some(unsafe { pos.unchecked_add(offset) });
            }

            // SAFETY: by while condition, `cursor` is valid until CHUNK_SIZE bytes
            cursor = unsafe { cursor.add(CHUNK_SIZE) };
        }

        self.find_iter(cursor, byte)
    }

    fn find_iter(&self, mut cursor: *const u8, byte: u8) -> Option<usize> {
        while cursor < self.end {
            // SAFETY: by while condition, `cursor` still in valid memory
            if unsafe { *cursor } == byte {
                // SAFETY: `cursor >= self.cursor`,
                // cursor only being `add`-ed, and `self.cursor` is unchanged
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
    // SAFETY: guaranteed by caller that `end >= start`,
    // and no need check from `usize::try_from`
    unsafe { end.offset_from(start) as usize }
}

/// Safely add pointer by casting to usize.
#[inline]
fn safe_add(ptr: *const u8, add: usize) -> usize {
    // SAFETY: pointer will never exceed `isize::MAX` so it would never overflow
    unsafe { (ptr as usize).unchecked_add(add) }
}

#[cfg(test)]
mod test {
    use super::*;

    const BUF: [u8; 23] = *b"Content-Type: text/html";
    const BUF_LEN: usize = BUF.len();

    const BUF2: [u8; 11] = *b": text/html";
    const BUF2_LEN: usize = BUF2.len();
    const BUF2_ADV: usize = BUF_LEN - BUF2_LEN;

    #[test]
    fn test_cursor_empty() {
        let mut cursor = Cursor::new(b"");

        assert_eq!(cursor.peek(), None);
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
        assert_eq!(cursor.peek_chunk::<0>(), Some(&[]));
        assert_eq!(cursor.peek_chunk::<2>(), BUF.first_chunk::<2>());
        assert_eq!(cursor.peek_chunk::<BUF_LEN>(), Some(&BUF));
        assert_eq!(cursor.peek_chunk::<{ BUF_LEN + 1 }>(), None);

        unsafe { cursor.advance(BUF2_ADV) };

        assert_eq!(cursor.peek(), BUF2.first().copied());
        assert_eq!(cursor.peek_chunk::<0>(), Some(&[]));
        assert_eq!(cursor.peek_chunk::<2>(), BUF2.first_chunk::<2>());
        assert_eq!(cursor.peek_chunk::<BUF2_LEN>(), Some(&BUF2));
        assert_eq!(cursor.peek_chunk::<{ BUF2_LEN + 1 }>(), None);
    }

    #[test]
    fn test_cursor_next() {
        let mut cursor = Cursor::new(&BUF[..]);

        assert_eq!(cursor.next_chunk::<0>(), Some(&[]));
        assert_eq!(cursor.as_bytes(), &BUF[..]);

        assert_eq!(cursor.next(), BUF.first().copied());
        assert_eq!(cursor.next_chunk::<2>(), BUF[1..].first_chunk::<2>());
        assert_eq!(cursor.next_chunk::<{ BUF_LEN - 3 }>(), BUF[3..].first_chunk::<{ BUF_LEN - 3 }>());
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
}
