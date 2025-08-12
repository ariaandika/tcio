use super::Bytes;

/// A cheaply cloneable and sliceable str.
///
/// An immutable [`String`] with storage backed by [`Bytes`].
#[derive(Clone)]
pub struct ByteStr {
    /// INVARIANT: bytes is a valid utf8
    bytes: Bytes,
}

impl ByteStr {
    /// Creates new empty [`ByteStr`].
    ///
    /// This function does not allocate.
    #[inline]
    pub const fn new() -> ByteStr {
        Self { bytes: Bytes::new() }
    }

    /// Converts a [`Bytes`] to a [`ByteStr`].
    ///
    /// # Errors
    ///
    /// Returns `Err` if the slice is not UTF-8 with a description as to why the provided slice is
    /// not UTF-8.
    #[inline]
    pub const fn from_utf8(bytes: Bytes) -> Result<Self, FromUtf8Error> {
        match str::from_utf8(bytes.as_slice()) {
            Ok(_) => Ok(Self { bytes }),
            Err(error) => Err(FromUtf8Error { error, bytes }),
        }
    }

    /// Create [`ByteStr`] from a slice of `str` that is equivalent to the given `bytes`.
    ///
    /// # Panics
    ///
    /// Requires that the given `sub` str is in fact contained within the `bytes` buffer;
    /// otherwise this function will panic.
    #[inline]
    pub fn from_slice_of(subset: &str, bytes: &Bytes) -> Self {
        Self { bytes: bytes.slice_from_raw(subset.as_ptr(), subset.len()) }
    }

    /// Converts a [`Bytes`] to a [`ByteStr`] without checking that the string contains valid
    /// UTF-8.
    ///
    /// # Safety
    ///
    /// The bytes passed in must be valid UTF-8.
    #[inline]
    pub const unsafe fn from_utf8_unchecked(bytes: Bytes) -> Self {
        debug_assert!(str::from_utf8(bytes.as_slice()).is_ok(), "`from_utf8_unchecked` receive non-UTF8");
        Self { bytes }
    }

    /// Creates [`ByteStr`] instance from str slice, by copying it.
    #[inline]
    pub fn copy_from_str(string: &str) -> Self {
        Self { bytes: Bytes::copy_from_slice(string.as_bytes()) }
    }

    /// Creates a new [`ByteStr`] from a static str.
    ///
    /// The returned [`ByteStr`] will point directly to the static str. There is
    /// no allocating or copying.
    #[inline]
    pub const fn from_static(string: &'static str) -> Self {
        Self { bytes: Bytes::from_static(string.as_bytes()) }
    }

    // /// Try to get mutable reference to underlying string.
    // ///
    // /// If `self` is not unique for the entire original buffer, callback not called and return `false`.
    // ///
    // /// # Examples
    // ///
    // /// ```
    // /// use tcio::ByteStr;
    // ///
    // /// let mut text = ByteStr::copy_from_str("Content-Type");
    // /// assert!(text.try_mut(|e|e.make_ascii_lowercase()));
    // /// assert_eq!(&text, "content-type");
    // /// ```
    // pub fn try_mut<F: FnOnce(&mut str)>(&mut self, f: F) -> bool {
    //     match Bytes::try_into_mut(std::mem::take(&mut self.bytes)) {
    //         Ok(mut original) => {
    //             // SAFETY: invariant bytes is a valid utf8
    //             let str_mut = unsafe { str::from_utf8_unchecked_mut(&mut original) };
    //             f(str_mut);
    //             self.bytes = original.freeze();
    //             true
    //         },
    //         Err(original) => {
    //             self.bytes = original;
    //             false
    //         },
    //     }
    // }

    /// Clears the string, removing all data.
    #[inline]
    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    /// Returns true if this is the only reference to the data.
    ///
    /// Always returns false if the data is backed by a static slice.
    #[inline]
    pub fn is_unique(&self) -> bool {
        self.bytes.is_unique()
    }

    /// Extracts a string slice containing the entire `ByteStr`.
    #[inline]
    pub const fn as_str(&self) -> &str {
        // SAFETY: invariant bytes is a valid utf8
        unsafe { str::from_utf8_unchecked(self.bytes.as_slice()) }
    }

    /// Returns a slice str of self that is equivalent to the given `subset`.
    ///
    /// This operation is `O(1)`.
    ///
    /// # Panics
    ///
    /// Requires that the given `sub` slice str is in fact contained within the
    /// `ByteStr` buffer; otherwise this function will panic.
    ///
    /// see also [`Bytes::slice_ref`]
    #[inline]
    pub fn slice_ref(&self, subset: &str) -> Self {
        Self { bytes: Bytes::slice_ref(&self.bytes, subset.as_bytes()) }
    }

    /// Convert [`ByteStr`] into [`String`].
    ///
    /// The bytes move/copy behavior is depends on [`Into<Vec>`] implementation of [`Bytes`].
    #[inline]
    pub fn into_string(self) -> String {
        // SAFETY: invariant bytes is a valid utf8
        unsafe { String::from_utf8_unchecked(Vec::from(self.bytes)) }
    }

    /// Converts a [`ByteStr`] into a [`Bytes`].
    #[inline]
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

// ===== Constructor =====
// everything should be constructed from a valid ut8

impl From<&'static str> for ByteStr {
    #[inline]
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

impl From<std::borrow::Cow<'static,str>> for ByteStr {
    #[inline]
    fn from(value: std::borrow::Cow<'static,str>) -> Self {
        match value {
            std::borrow::Cow::Borrowed(s) => Self::from(s),
            std::borrow::Cow::Owned(s) => Self::from(s),
        }
    }
}

impl From<Box<str>> for ByteStr {
    #[inline]
    fn from(value: Box<str>) -> Self {
        Self { bytes: Bytes::from(value.into_boxed_bytes()) }
    }
}

impl From<String> for ByteStr {
    #[inline]
    fn from(value: String) -> Self {
        Self { bytes: Bytes::from(value.into_bytes()) }
    }
}

impl Default for ByteStr {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// ===== Destructor =====

impl From<ByteStr> for Bytes {
    #[inline]
    fn from(value: ByteStr) -> Self {
        value.bytes
    }
}

impl From<ByteStr> for String {
    #[inline]
    fn from(value: ByteStr) -> Self {
        value.into_string()
    }
}

// ===== References =====

impl AsRef<[u8]> for ByteStr {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsRef<str> for ByteStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ByteStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_str(), f)
    }
}

crate::macros::impl_std_traits! {
    impl ByteStr;

    fn deref(&self) -> &str { self.as_str() }

    fn fmt(&self: ByteStr, f) { str::fmt(self, f) }

    fn eq(&self, &other: str) { str::eq(self, other) }
    fn eq(&self, &other: &str) { str::eq(self, *other) }
    fn eq(&self, &other: String) { str::eq(self, other) }
    fn eq(&self, &other: ByteStr) { str::eq(self, other.as_str()) }
}

// ===== Error =====

/// A possible error value when converting a `String` from a UTF-8 byte vector.
pub struct FromUtf8Error {
    bytes: Bytes,
    error: std::str::Utf8Error,
}

impl FromUtf8Error {
    /// Fetch a `Utf8Error` to get more details about the conversion failure.
    #[inline]
    pub const fn utf8_error(&self) -> &std::str::Utf8Error {
        &self.error
    }

    /// Returns a slice of [`u8`]s bytes that were attempted to convert to a `ByteStr`.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Returns the bytes that were attempted to convert to a `ByteStr`.
    #[inline]
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

impl std::error::Error for FromUtf8Error { }

impl std::fmt::Debug for FromUtf8Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::fmt::Display for FromUtf8Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

