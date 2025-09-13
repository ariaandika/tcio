use core::slice::from_raw_parts;

/// Bytes cursor.
///
/// This is intended to be used for interpreting bytes with unsafe unchecked bounds.
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
    /// Creates new [`Cursor`] from initialized bytes.
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

    /// Creates new [`Cursor`] with starting cursor at the end of the buffer.
    ///
    /// Following [`next()`][Cursor::next] call will returns [`None`].
    ///
    /// This is used when caller wants to iterate backwards.
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

    // ===== Reference =====

    /// Returns how many cursor has stepped forward.
    #[inline]
    pub const fn steps(&self) -> usize {
        // SAFETY: invariant `self.start <= self.cursor`
        unsafe { self.cursor.offset_from_unsigned(self.start) }
    }

    /// Returns the remaining length of the bytes.
    #[inline]
    pub const fn remaining(&self) -> usize {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { self.end.offset_from_unsigned(self.cursor) }
    }

    /// Returns `true` if there is at least one remaining bytes.
    #[inline]
    pub const fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns reference to the original bytes.
    #[inline]
    pub const fn original(&self) -> &'a [u8] {
        // SAFETY: invariant `self.start <= self.end`
        unsafe { from_raw_parts(self.start, self.end.offset_from_unsigned(self.start)) }
    }

    /// Returns reference to the already advanced slice.
    #[inline]
    pub const fn advanced_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { from_raw_parts(self.start, self.steps()) }
    }

    /// Returns reference to the remaining bytes.
    #[inline]
    pub const fn as_slice(&self) -> &'a [u8] {
        // SAFETY: invariant `self.cursor <= self.end`
        unsafe { from_raw_parts(self.cursor, self.remaining()) }
    }

    /// Returns the pointer this cursor point to.
    #[inline]
    pub const fn as_ptr(&self) -> *const u8 {
        self.cursor
    }

    // ===== Peek =====

    /// Returns the next byte without advancing the cursor.
    ///
    /// Returns [`None`] if there is no remaining bytes.
    #[inline]
    pub const fn peek(&self) -> Option<u8> {
        if self.has_remaining() {
            // SAFETY: `has_remaining()` returns `true`
            Some(unsafe { self.peek_unchecked() })
        } else {
            None
        }
    }

    /// Returns the next byte without advancing the cursor.
    ///
    /// # Safety
    ///
    /// Cursor must have at least one remaining bytes.
    ///
    /// This method is safe to call if one of the following conditions is met:
    ///
    /// - [`has_remaining()`] returns `true`
    /// - [`remaining()`] returns more than or equal to `1`.
    ///
    /// [`has_remaining()`]: Self::has_remaining
    /// [`remaining()`]: Self::remaining
    #[inline]
    pub const unsafe fn peek_unchecked(&self) -> u8 {
        debug_assert!(self.has_remaining(), "safety violated, out of bounds");

        // SAFETY: caller must ensure that there at least one remaining.
        unsafe { *self.cursor }
    }

    /// Returns the next `n`-th byte without advancing the cursor.
    ///
    /// Returns [`None`] if `n` is more than or equal to remaining bytes.
    ///
    /// The count starts from zero, so `peek_nth(0)` returns the first value, `peek_nth(1)` the
    /// second, and so on.
    #[inline]
    pub const fn peek_nth(&self, n: usize) -> Option<u8> {
        if n < self.remaining() {
            // SAFETY: `self.cursor` is valid until `n` forward
            Some(unsafe { *self.cursor.add(n) })
        } else {
            None
        }
    }

    /// Returns the next `n`-th byte without advancing the cursor.
    ///
    /// # Safety
    ///
    /// `n` must be less than remaining bytes.
    #[inline]
    pub const unsafe fn peek_nth_unchecked(&self, n: usize) -> u8 {
        debug_assert!(n < self.remaining(), "safety violated, out of bounds");

        // SAFETY: caller must ensure that there at least `n` remaining.
        unsafe { *self.cursor.add(n) }
    }

    /// Returns reference to the next `N` bytes without advancing the cursor.
    ///
    /// Returns [`None`] if `N` is more than remaining bytes.
    #[inline]
    pub const fn peek_chunk<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if N <= self.remaining() {
            // SAFETY: `self.cursor` is valid until `N` bytes
            Some(unsafe { &*self.cursor.cast() })
        } else {
            None
        }
    }

    // ===== Peek Prev =====

    /// Returns the previous byte without stepping back the cursor.
    ///
    /// Returns [`None`] if cursor is at the beginning of the bytes.
    #[inline]
    pub const fn peek_prev(&self) -> Option<u8> {
        if self.steps() != 0 {
            // SAFETY: already advance once
            Some(unsafe { *self.cursor.sub(1) })
        } else {
            None
        }
    }

    /// Returns references to the previous `N` bytes without stepping back the cursor.
    ///
    /// Returns [`None`] if `N` is more than [`steps()`][Self::steps].
    #[inline]
    pub const fn peek_prev_chunk<const N: usize>(&self) -> Option<&'a [u8; N]> {
        if N <= self.steps() {
            // SAFETY: already advanced `N`-th
            Some(unsafe { &*self.cursor.sub(N).cast() })
        } else {
            None
        }
    }

    // ===== Next =====

    /// Returns the next byte and advance the cursor.
    ///
    /// Returns [`None`] if there is no remaining bytes.
    #[inline]
    #[allow(
        clippy::should_implement_trait,
        reason = "specialized Iterator, see note below"
    )]
    pub const fn next(&mut self) -> Option<u8> {
        // no impl Iterator, though this IS an Iterator, but all the method is optimized for bytes,
        // so callers could be mistaken to call the blanket method from Iterator trait

        if self.has_remaining() {
            // SAFETY: `has_remaining()` returns `true`
            Some(unsafe { self.next_unchecked() })
        } else {
            None
        }
    }

    /// Returns the next byte and advance the cursor.
    ///
    /// # Safety
    ///
    /// Cursor must have at least one remaining bytes.
    ///
    /// This method is safe to call if one of the following conditions is met:
    ///
    /// - [`has_remaining()`] returns `true`
    /// - [`remaining()`] returns more than or equal to `1`.
    ///
    /// [`has_remaining()`]: Self::has_remaining
    /// [`remaining()`]: Self::remaining
    #[inline]
    pub const unsafe fn next_unchecked(&mut self) -> u8 {
        debug_assert!(self.has_remaining(), "safety violated, out of bounds");

        // SAFETY: caller must ensure that there at least one remaining.
        unsafe {
            let val = *self.cursor;
            self.advance_unchecked(1);
            val
        }
    }

    /// Returns references to the next `N` bytes and advance the cursor by `N`.
    ///
    /// Returns [`None`] if `N` is more than remaining bytes.
    #[inline]
    pub const fn next_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        if N <= self.remaining() {
            // SAFETY: `self.cursor` is valid until `N` bytes
            unsafe {
                let val = &*self.cursor.cast();
                self.advance_unchecked(N);
                Some(val)
            }
        } else {
            None
        }
    }

    // ===== Prev =====

    /// Returns the previous byte and step back the cursor.
    ///
    /// Returns [`None`] if cursor is at the beginning of the bytes.
    #[inline]
    pub const fn prev(&mut self) -> Option<u8> {
        if self.steps() != 0 {
            // SAFETY: already advance once
            unsafe {
                self.step_back_unchecked(1);
                Some(*self.cursor)
            }
        } else {
            None
        }
    }

    /// Returns references to the previous `N` bytes and step back the cursor by `N`.
    ///
    /// Returns [`None`] if `N` is more than [`steps()`][Self::steps].
    #[inline]
    pub const fn prev_chunk<const N: usize>(&mut self) -> Option<&'a [u8; N]> {
        if N <= self.steps() {
            // SAFETY: already advanced `N`
            unsafe {
                self.step_back_unchecked(N);
                Some(&*self.cursor.cast())
            }
        } else {
            None
        }
    }

    // ===== Advance / Step Back =====

    /// Advance cursor forward by `n`.
    ///
    /// # Panics
    ///
    /// `n` must be less than or equal to remaining bytes, otherwise panic.
    #[inline]
    pub const fn advance(&mut self, n: usize) {
        assert!(n <= self.remaining(), "out of bounds");

        // SAFETY: asserted
        unsafe { self.cursor = self.cursor.add(n); }
    }

    /// Advance cursor forward by `n`.
    ///
    /// # Safety
    ///
    /// `n` must be less than or equal to remaining bytes.
    #[inline]
    pub const unsafe fn advance_unchecked(&mut self, n: usize) {
        debug_assert!(n <= self.remaining(), "safety violated, out of bounds");

        // SAFETY: caller must ensure that `n` is in bounds
        unsafe { self.cursor = self.cursor.add(n); }
    }

    /// Advance cursor to the end.
    ///
    /// This is used when caller wants to iterate backwards.
    #[inline]
    pub const fn advance_to_end(&mut self) {
        self.cursor = self.end;
    }

    /// Move cursor backwards by `n`.
    ///
    /// # Panics
    ///
    /// `n` must be less than or equal to [`steps()`][Self::steps], otherwise panic.
    #[inline]
    pub const fn step_back(&mut self, n: usize) {
        assert!(n <= self.steps(), "out of bounds");

        // SAFETY: assertion
        unsafe { self.cursor = self.cursor.sub(n); }
    }

    /// Move cursor backwards by `n`.
    ///
    /// # Safety
    ///
    /// `n` must be less than or equal to [`steps()`][Self::steps].
    #[inline]
    pub const unsafe fn step_back_unchecked(&mut self, n: usize) {
        debug_assert!(n <= self.steps(), "safety violated, out of bounds");

        // SAFETY: caller must ensure that that stepping back is in bound of the slice
        unsafe { self.cursor = self.cursor.sub(n); }
    }

    /// Take current cursor to `len` of the slice.
    ///
    /// If `len` is more than remaining bytes, this is a noop.
    #[inline]
    pub const fn truncate(&mut self, len: usize) {
        if len < self.remaining() {
            // SAFETY: still in bounds of `self.cursor` and `self.end`
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

    // ===== Split =====

    /// Returns the first and all the rest of the bytes.
    ///
    /// This can be used after cursor found a delimiter.
    ///
    /// # Safety
    ///
    /// Cursor must have at least one remaining bytes.
    ///
    /// If [`has_remaining`] returns `true`, this method is safe to call.
    ///
    /// [`has_remaining`]: Self::has_remaining
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Cursor;
    /// let mut cursor = Cursor::new(b"user:pass@example.com");
    ///
    /// while let Some(byte) = cursor.peek() {
    ///     if byte == b'@' {
    ///         break;
    ///     } else {
    ///         cursor.advance(1);
    ///     }
    /// }
    ///
    /// assert!(cursor.has_remaining());
    ///
    /// // SAFETY: `cursor.has_remaining()` returns `true`
    /// let (delim, host) = unsafe { cursor.split_first() };
    /// let userinfo = cursor.advanced_slice();
    ///
    /// assert_eq!(delim, b'@');
    /// assert_eq!(userinfo, b"user:pass");
    /// assert_eq!(host, b"example.com");
    /// ```
    pub const unsafe fn split_first(&self) -> (u8, &'a [u8]) {
        debug_assert!(self.has_remaining());

        // SAFETY: user must ensure that there is at least one remaining byte
        unsafe {
            let rest = self.cursor.add(1);
            let rest_len = self.end.offset_from_unsigned(rest);

            (*self.cursor, from_raw_parts(rest, rest_len))
        }
    }

    /// Returns the last and all the rest of the advanced bytes.
    ///
    /// This can be used after cursor found a delimiter.
    ///
    /// # Safety
    ///
    /// Cursor must have at least one remaining bytes.
    ///
    /// If [`steps()`] does not returns `0`, this method is safe to call.
    ///
    /// [`steps()`]: Self::steps
    ///
    /// # Examples
    ///
    /// ```
    /// # use tcio::bytes::Cursor;
    /// let mut cursor = Cursor::new(b"user:pass@example.com");
    ///
    /// while let Some(byte) = cursor.next() {
    ///     if byte == b'@' {
    ///         break;
    ///     }
    /// }
    ///
    /// assert!(cursor.steps() != 0);
    ///
    /// // SAFETY: `cursor.steps()` does not returns `0`
    /// let (delim, userinfo) = unsafe { cursor.split_last_advanced() };
    /// let host = cursor.as_slice();
    ///
    /// assert_eq!(delim, b'@');
    /// assert_eq!(userinfo, b"user:pass");
    /// assert_eq!(host, b"example.com");
    /// ```
    pub const unsafe fn split_last_advanced(&self) -> (u8, &'a [u8]) {
        debug_assert!(self.steps() != 0);

        // SAFETY: user must ensure that cursor have advanced at least once
        unsafe {
            let at = self.cursor.sub(1);
            let rest_len = at.offset_from_unsigned(self.start);

            (*at, from_raw_parts(self.start, rest_len))
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
