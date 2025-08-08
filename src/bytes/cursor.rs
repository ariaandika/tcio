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
        unsafe { std::slice::from_raw_parts(self.start, self.end.offset_from(self.start) as _) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { std::slice::from_raw_parts(self.cursor, self.remaining()) }
    }

    /// Returns the already advanced slice.
    #[inline]
    pub fn advanced_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { std::slice::from_raw_parts(self.start, self.cursor.offset_from(self.start) as _) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { std::slice::from_raw_parts(self.cursor, self.remaining()) }
    }

    /// Returns the pointer this cursor point to.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.cursor
    }

    // ===== Peek =====

    /// Try get the first byte without advancing cursor.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        if self.cursor == self.end {
            None
        } else {
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
        // so callers could be mistaken to call the blanket method from Iterator trait

        if self.cursor == self.end {
            None
        } else {
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
    }

    /// Move cursor backwards cursor.
    ///
    /// # Panics
    ///
    /// Panic if step back pass the first byte.
    #[inline]
    pub fn step_back(&mut self, n: usize) {
        unsafe {
            assert!(
                // SAFETY: invariant `self.start <= self.cursor`
                self.cursor.offset_from(self.start) as usize >= n,
                "`Cursor::step_back` out of bounds"
            );
            self.cursor = self.cursor.sub(n);
        }
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
