use super::{Bytes, BytesMut};

/// Returns the pointer range of a buffer.
///
/// This is intended to be used with [`slice_of_bytes`] to keep a slice of [`BytesMut`] and
/// freezing it while keeping the slice without copying.
///
/// # Examples
///
/// ```
/// # use bytes::{BytesMut, Bytes};
/// use tcio::bytes::{range_of, slice_of_bytes};
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
/// use tcio::bytes::{range_of, slice_of_bytes};
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
/// use tcio::bytes::{range_of, slice_of_bytes};
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
        return Bytes::new();
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
/// use tcio::bytes::{range_of, slice_of_bytes_mut};
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
    crate::bytes::Buf::advance(bytes, leading_len);

    bytes.split_to(sub_len)
}

/// Returns the subset value in `buf` with returned range from [`range_of`].
///
/// # Examples
///
/// ```
/// use tcio::bytes::{range_of, slice_of};
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
        return &[];
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
