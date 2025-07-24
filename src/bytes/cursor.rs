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
        unsafe { self.cursor.offset_from(self.start) as _ }
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub fn remaining(&self) -> usize {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { self.end.offset_from(self.cursor) as _ }
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
        unsafe { slice(self.start, self.end.offset_from(self.start) as _) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { slice(self.cursor, self.remaining()) }
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
        if self.remaining() >= N {
            // SAFETY: `self.cursor` is valid until `N` bytes
            Some(unsafe { &*self.cursor.cast() })
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
        if self.remaining() >= N {
            // SAFETY: `self.cursor` is valid until `N` bytes
            unsafe {
                let val = &*self.cursor.cast();
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
    pub fn next_find<S: Strategy>(&mut self, strategy: S) -> Option<&'a [u8]> {
        match self.find_inner(strategy) {
            // SAFETY: checked by Strategy implementation
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
    pub fn next_until<S: Strategy>(&mut self, strategy: S) -> Option<&'a [u8]> {
        match self.find_inner(strategy) {
            // SAFETY: checked by Strategy implementation
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
    pub fn next_split<S: Strategy>(&mut self, strategy: S) -> Option<&'a [u8]> {
        match self.find_inner(strategy) {
            // SAFETY: checked by Strategy implementation
            Some(len) => unsafe {
                let chunk = slice(self.cursor, len);
                self.advance(len + 1);
                Some(chunk)
            },
            None => None,
        }
    }

    #[inline]
    fn find_inner<S: Strategy>(&self, strategy: S) -> Option<usize> {
        if self.remaining() < size_of::<usize>() {
            S::find_iter(strategy, self.cursor, self)
        } else {
            S::find_swar(strategy, self)
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
            self.remaining() >= n,
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
            // SAFETY: invariant `self.start <= self.cursor`
            unsafe { self.cursor.offset_from(self.start) } as usize >= n,
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
    #[inline]
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
}

/// Byte finding strategy.
pub trait Strategy: sealed::Sealed { }

mod sealed {
    use std::ops::RangeFull;
    use super::*;

    const CHUNK_SIZE: usize = size_of::<usize>();
    const LSB: usize = usize::from_ne_bytes([1; CHUNK_SIZE]);
    const MSB: usize = usize::from_ne_bytes([128; CHUNK_SIZE]);

    pub trait Sealed: Sized {
        fn find_iter(self, start: *const u8, cursor: &Cursor<'_>) -> Option<usize>;

        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize>;
    }

    // ===== Single u8 =====

    impl Strategy for u8 { }
    impl Sealed for u8 {
        fn find_iter(self, mut current: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            while current < cursor.end {
                // SAFETY: by while condition, `cursor` still in valid memory
                if unsafe { *current } == self {
                    // SAFETY: `cursor >= cursor.cursor`,
                    // cursor only being `add`-ed, and `cursor.cursor` is unchanged
                    return Some(unsafe { current.offset_from(cursor.cursor) as usize });
                } else {
                    // SAFETY: Because allocated objects can never be larger than `isize::MAX` bytes,
                    // `cursor == cursor.end` is always safe
                    current = unsafe { current.add(1) };
                }
            }

            None
        }

        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            let target = usize::from_ne_bytes([self; CHUNK_SIZE]);
            let mut current = cursor.cursor;

            // SAFETY: `current` is always checked when set that is `<= cursor.end`
            while unsafe { cursor.end.offset_from(current) as usize } > CHUNK_SIZE {
                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                let x = usize::from_ne_bytes(unsafe { *current.cast() });

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

                    // SAFETY: `current >= cursor.cursor`,
                    // `current` only `add`-ed, and `cursor.cursor` is unchanged
                    let offset = unsafe { current.offset_from(cursor.cursor) as usize };

                    // SAFETY: pointer will never exceed `isize::MAX` so `usize` would never overflow
                    return Some(unsafe { pos.unchecked_add(offset) });
                }

                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                current = unsafe { current.add(CHUNK_SIZE) };
            }

            self.find_iter(current, cursor)
        }
    }

    // ===== Double u8 =====

    impl Strategy for (u8,u8) { }
    impl Sealed for (u8,u8) {
        fn find_iter(self, mut current: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            while current < cursor.end {
                // SAFETY: by while condition, `cursor` still in valid memory
                let c = unsafe { *current };
                if c == self.0 || c == self.1 {
                    // SAFETY: `current >= cursor.cursor`,
                    // current only being `add`-ed, and `cursor.cursor` is unchanged
                    return Some(unsafe { current.offset_from(cursor.cursor) as usize });
                } else {
                    // SAFETY: Because allocated objects can never be larger than `isize::MAX` bytes,
                    // `current == cursor.end` is always safe
                    current = unsafe { current.add(1) };
                }
            }

            None
        }

        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            let t1 = usize::from_ne_bytes([self.0; CHUNK_SIZE]);
            let t2 = usize::from_ne_bytes([self.1; CHUNK_SIZE]);

            let mut current = cursor.cursor;

            // SAFETY: `current` is always checked when set that is `<= cursor.end`
            while unsafe { cursor.end.offset_from(current) as usize } > CHUNK_SIZE {
                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                let value = usize::from_ne_bytes(unsafe { *current.cast() });

                // SWAR
                // explanation in `u8` implementation of `Strategy`
                //
                // additionally, we OR to merge 2 result

                let xor_1 = value ^ t1;
                let f1 = xor_1.wrapping_sub(LSB) & !xor_1;

                let xor_2 = value ^ t2;
                let f2 = xor_2.wrapping_sub(LSB) & !xor_2;

                let found = (f1 | f2) & MSB;

                if found != 0 {
                    let pos = (found.trailing_zeros() / 8) as usize;

                    // SAFETY: `current >= cursor.cursor`,
                    // `current` only `add`-ed, and `cursor.cursor` is unchanged
                    let offset = unsafe { current.offset_from(cursor.cursor) as usize };

                    // SAFETY: pointer will never exceed `isize::MAX` so `usize` would never overflow
                    return Some(unsafe { pos.unchecked_add(offset) });
                }

                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                current = unsafe { current.add(CHUNK_SIZE) };
            }

            self.find_iter(current, cursor)
        }
    }

    // ===== Triple u8 =====

    impl Strategy for (u8,u8,u8) { }
    impl Sealed for (u8,u8,u8) {
        fn find_iter(self, mut current: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            while current < cursor.end {
                // SAFETY: by while condition, `cursor` still in valid memory
                let c = unsafe { *current };
                if c == self.0 || c == self.1 || c == self.2 {
                    // SAFETY: `current >= cursor.cursor`,
                    // current only being `add`-ed, and `cursor.cursor` is unchanged
                    return Some(unsafe { current.offset_from(cursor.cursor) as usize });
                } else {
                    // SAFETY: Because allocated objects can never be larger than `isize::MAX` bytes,
                    // `current == cursor.end` is always safe
                    current = unsafe { current.add(1) };
                }
            }

            None
        }

        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            let t1 = usize::from_ne_bytes([self.0; CHUNK_SIZE]);
            let t2 = usize::from_ne_bytes([self.1; CHUNK_SIZE]);
            let t3 = usize::from_ne_bytes([self.2; CHUNK_SIZE]);

            let mut current = cursor.cursor;

            // SAFETY: `current` is always checked when set that is `<= cursor.end`
            while unsafe { cursor.end.offset_from(current) as usize } > CHUNK_SIZE {
                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                let value = usize::from_ne_bytes(unsafe { *current.cast() });

                // SWAR
                // explanation in `u8` implementation of `Strategy`
                //
                // additionally, we OR to merge 3 result

                let xor_1 = value ^ t1;
                let f1 = xor_1.wrapping_sub(LSB) & !xor_1;

                let xor_2 = value ^ t2;
                let f2 = xor_2.wrapping_sub(LSB) & !xor_2;

                let xor_3 = value ^ t3;
                let f3 = xor_3.wrapping_sub(LSB) & !xor_3;

                let found = (f1 | f2 | f3) & MSB;

                if found != 0 {
                    let pos = (found.trailing_zeros() / 8) as usize;

                    // SAFETY: `current >= cursor.cursor`,
                    // `current` only `add`-ed, and `cursor.cursor` is unchanged
                    let offset = unsafe { current.offset_from(cursor.cursor) as usize };

                    // SAFETY: pointer will never exceed `isize::MAX` so `usize` would never overflow
                    return Some(unsafe { pos.unchecked_add(offset) });
                }

                // SAFETY: by while condition, `current` is valid until CHUNK_SIZE bytes
                current = unsafe { current.add(CHUNK_SIZE) };
            }

            self.find_iter(current, cursor)
        }
    }

    // ===== Other =====

    impl Strategy for (u8,) { }
    impl Sealed for (u8,) {
        #[inline]
        fn find_iter(self, start: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            self.0.find_iter(start, cursor)
        }

        #[inline]
        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            self.0.find_swar(cursor)
        }
    }

    /// Force to use iteration approach, this is used when user sure that finding will be short.
    impl Strategy for (u8, RangeFull) { }
    impl Sealed for (u8, RangeFull) {
        #[inline]
        fn find_iter(self, start: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            self.0.find_iter(start, cursor)
        }

        #[inline]
        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            self.0.find_iter(cursor.cursor, cursor)
        }
    }

    /// Force to use iteration approach, this is used when user sure that finding will be short.
    impl Strategy for (u8, u8, RangeFull) { }
    impl Sealed for (u8, u8, RangeFull) {
        #[inline]
        fn find_iter(self, start: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            (self.0, self.1).find_iter(start, cursor)
        }

        #[inline]
        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            (self.0, self.1).find_iter(cursor.cursor, cursor)
        }
    }

    /// Force to use iteration approach, this is used when user sure that finding will be short.
    impl Strategy for (u8, u8, u8, RangeFull) { }
    impl Sealed for (u8, u8, u8, RangeFull) {
        #[inline]
        fn find_iter(self, start: *const u8, cursor: &Cursor<'_>) -> Option<usize> {
            (self.0, self.1, self.2).find_iter(start, cursor)
        }

        #[inline]
        fn find_swar(self, cursor: &Cursor<'_>) -> Option<usize> {
            (self.0, self.1, self.2).find_iter(cursor.cursor, cursor)
        }
    }
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
        const BUF: [u8; 23] = *b"Content-Type: text/html";

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find(b'-'), Some(&b"Content"[..]));
        assert_eq!(cursor.as_bytes(), &b"-Type: text/html"[..]);

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find(b':'), Some(&b"Content-Type"[..]));
        assert_eq!(cursor.as_bytes(), &b": text/html"[..]);

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find(b'*'), None);
        assert_eq!(cursor.as_bytes(), &BUF);

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find((b'-', b'T')), Some(&b"Content"[..]));
        assert_eq!(cursor.as_bytes(), &b"-Type: text/html"[..]);

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find((b'T', b'-')), Some(&b"Content"[..]));
        assert_eq!(cursor.as_bytes(), &b"-Type: text/html"[..]);

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_find((b'T', b'-', ..)), Some(&b"Content"[..]));
        assert_eq!(cursor.as_bytes(), &b"-Type: text/html"[..]);

        // until

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_until(b'-'), Some(&b"Content-"[..]));
        assert_eq!(cursor.as_bytes(), &b"Type: text/html"[..]);

        // split

        let mut cursor = Cursor::new(&BUF);
        assert_eq!(cursor.next_split(b'-'), Some(&b"Content"[..]));
        assert_eq!(cursor.as_bytes(), &b"Type: text/html"[..]);
    }
}
