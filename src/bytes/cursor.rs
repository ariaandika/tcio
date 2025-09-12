/// Raw bytes cursor.
///
/// Provides an API for interpreting bytes.
//
// INVARIANT: self.start <= self.cursor <= self.end
//
// note that even if `self.cursor == self.end`, dereferencing to slice would returns empty slice.
#[derive(Debug, Clone)]
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
    pub const fn new(buf: &'a [u8]) -> Self {
        Self {
            start: buf.as_ptr(),
            cursor: buf.as_ptr(),
            // SAFETY: allocated objects can never be larger than `isize::MAX` bytes,
            // `self.cursor == self.end` is always safe
            end: unsafe { buf.as_ptr().add(buf.len()) },
            _p: std::marker::PhantomData,
        }
    }

    /// Create new [`Cursor`] with starting cursor at the end of the buffer.
    ///
    /// Following [`next()`][Cursor::next] call will returns [`None`].
    ///
    /// This is usefull when callers want to iterate backwards.
    #[inline]
    pub const fn from_end(buf: &'a [u8]) -> Self {
        // SAFETY: allocated objects can never be larger than `isize::MAX` bytes,
        // `self.cursor == self.end` is always safe
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        Self {
            start: buf.as_ptr(),
            cursor: end,
            end,
            _p: std::marker::PhantomData,
        }
    }

    /// Workaround for self referencing struct.
    ///
    /// The callers must not expose mutable reference of given buffer while `Cursor` is not yet
    /// dropped.
    pub(crate) const fn new_unbound(buf: &[u8]) -> Self {
        Self {
            start: buf.as_ptr(),
            cursor: buf.as_ptr(),
            // SAFETY: allocated objects can never be larger than `isize::MAX` bytes,
            // `self.cursor == self.end` is always safe
            end: unsafe { buf.as_ptr().add(buf.len()) },
            _p: std::marker::PhantomData,
        }
    }

    /// Workaround for self referencing struct.
    ///
    /// The callers must not expose mutable reference of given buffer while `Cursor` is not yet
    /// dropped.
    pub(crate) const fn from_end_unbound(buf: &[u8]) -> Self {
        // SAFETY: allocated objects can never be larger than `isize::MAX` bytes,
        // `self.cursor == self.end` is always safe
        let end = unsafe { buf.as_ptr().add(buf.len()) };
        Self {
            start: buf.as_ptr(),
            cursor: end,
            end,
            _p: std::marker::PhantomData,
        }
    }

    /// Take current cursor to `len` of the slice.
    ///
    /// If `len` is more than slice length, the length is saturated.
    #[inline]
    pub const fn take(mut self, len: usize) -> Self {
        self.truncate(len);
        self
    }

    // ===== Reference =====

    /// Returns how many [`Cursor`] has stepped forward.
    #[inline]
    pub const fn steps(&self) -> usize {
        // SAFETY: invariant `self.start <= self.cursor`
        unsafe { self.cursor.offset_from_unsigned(self.start) }
    }

    /// Returns the remaining bytes length.
    #[inline]
    pub const fn remaining(&self) -> usize {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { self.end.offset_from_unsigned(self.cursor) }
    }

    /// Returns `true` if there is more bytes left.
    #[inline]
    pub const fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns the original bytes.
    #[inline]
    pub const fn original(&self) -> &'a [u8] {
        // SAFETY: invariant `self.start <= self.end`
        unsafe { core::slice::from_raw_parts(self.start, self.end.offset_from_unsigned(self.start)) }
    }

    /// Returns the already advanced slice.
    #[inline]
    pub const fn advanced_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { core::slice::from_raw_parts(self.start, self.steps()) }
    }

    /// Returns the remaining bytes.
    #[inline]
    pub const fn as_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { core::slice::from_raw_parts(self.cursor, self.remaining()) }
    }

    /// Returns the pointer this cursor point to.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.cursor
    }

    // ===== Peek =====

    /// Try get the first byte without advancing cursor.
    #[inline]
    pub const fn peek(&self) -> Option<u8> {
        if self.has_remaining() {
            // SAFETY: start is still in bounds
            Some(unsafe { *self.cursor })
        } else {
            None
        }
    }

    /// Try get the `n`-th byte without advancing cursor.
    ///
    /// the count starts from zero, so `nth(0)` returns the first value, `nth(1)` the second, and
    /// so on.
    #[inline]
    pub const fn peek_nth(&self, n: usize) -> Option<u8> {
        if self.remaining() > n {
            // SAFETY: `self.cursor` is valid until `n` forward
            Some(unsafe { *self.cursor.add(n) })
        } else {
            None
        }
    }

    /// Try get the first `N`-th bytes without advancing cursor.
    #[inline]
    pub const fn peek_chunk<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if self.remaining() >= N {
            // SAFETY: `self.cursor` is valid until `N` bytes
            Some(unsafe { &*self.cursor.cast() })
        } else {
            None
        }
    }

    /// Try get the previous bytes without stepping back cursor.
    #[inline]
    pub const fn peek_prev(&self) -> Option<u8> {
        if self.steps() > 0 {
            // SAFETY: already advance once
            Some(unsafe { *self.cursor.sub(1) })
        } else {
            None
        }
    }

    /// Try get the previous `N`-th bytes without stepping back cursor.
    #[inline]
    pub const fn peek_prev_chunk<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if self.steps() >= N {
            // SAFETY: already advanced `N`-th
            Some(unsafe { &*self.cursor.sub(N).cast() })
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
    pub const fn next(&mut self) -> Option<u8> {
        // no impl Iterator, though this IS an Iterator, but all the method is optimized for bytes,
        // so callers could be mistaken to call the blanket method from Iterator trait

        if self.has_remaining() {
            // SAFETY: `self.cursor` is still in bounds
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
    pub const fn next_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
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

    // ===== Prev =====

    /// Try get the previous byte and step back the cursor by `1`.
    #[inline]
    pub const fn prev(&mut self) -> Option<u8> {
        if self.steps() > 0 {
            // SAFETY: already advance once
            let val = unsafe { *self.cursor.sub(1) };
            self.step_back(1);
            Some(val)
        } else {
            None
        }
    }

    /// Try get the previous `N`-th bytes and step back cursor by `N`.
    #[inline]
    pub const fn prev_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        if self.steps() >= N {
            self.step_back(N);
            // SAFETY: already advanced `N`
            Some(unsafe { &*self.cursor.cast() })
        } else {
            None
        }
    }

    // ===== Advance / Step Back =====

    /// Advance cursor forward.
    ///
    /// # Panics
    ///
    /// Panic if advancing pass slice length.
    #[inline]
    pub const fn advance(&mut self, n: usize) {
        assert!(self.remaining() >= n, "Cursor::advance out of bounds");

        // SAFETY: asserted
        unsafe { self.cursor = self.cursor.add(n); }
    }

    /// Advance cursor to the end.
    ///
    /// This is usefull when callers want to iterate backwards.
    #[inline]
    pub const fn advance_to_end(&mut self) {
        self.cursor = self.end;
    }

    /// Move cursor backwards cursor.
    ///
    /// # Panics
    ///
    /// Panic if step back pass the first byte.
    #[inline]
    pub const fn step_back(&mut self, n: usize) {
        assert!(self.steps() >= n, "Cursor::step_back out of bounds");

        // SAFETY: assertion
        unsafe { self.cursor = self.cursor.sub(n); }
    }

    /// Take current cursor to `len` of the slice.
    ///
    /// If `len` is more than slice length, the length is saturated.
    #[inline]
    pub const fn truncate(&mut self, len: usize) {
        if self.remaining() > len {
            self.end = unsafe { self.cursor.add(len) };
        }
    }

    /// Clone the cursor.
    ///
    /// This method exists for const context.
    #[inline]
    pub const fn fork(&self) -> Self {
        Self {
            start: self.start,
            cursor: self.cursor,
            end: self.end,
            _p: std::marker::PhantomData,
        }
    }
}

impl<'a> From<&'a [u8]> for Cursor<'a> {
    #[inline]
    fn from(value: &'a [u8]) -> Self {
        Cursor::new(value)
    }
}

impl<'a> From<&'a super::Bytes> for Cursor<'a> {
    #[inline]
    fn from(value: &'a super::Bytes) -> Self {
        value.cursor()
    }
}

impl<'a> From<&'a super::BytesMut> for Cursor<'a> {
    #[inline]
    fn from(value: &'a super::BytesMut) -> Self {
        value.cursor()
    }
}
