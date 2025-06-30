use bytes::Bytes;

macro_rules! example_range {
    () => {
r#"# Examples

```
# use bytes::{BytesMut, Bytes};
use tcio::{range_of, slice_of_bytes};

let mut bytesm = BytesMut::from(&b"Content-Type: text/html"[..]);
let range = range_of(&bytesm[14..]);

// `.freeze()` require mutation on `bytesm`,
// so any slice reference cannot pass this point;
//
// because `.split_off()` guarantees that address
// does not change, its possible to store pointer as index,
// therefore, no lifetime restriction
let bytes = bytesm.split_off(12).freeze();

// Shared `Bytes`, no copy
let content: Bytes = slice_of_bytes(range, &bytes);

assert_eq!(content, &b"text/html"[..]);
```"#
    };
}

/// Returns the pointer range of a buffer.
///
/// This is intended to be used with [`slice_of_bytes`] to keep a slice of [`BytesMut`] and
/// freezing it while keeping the slice without copying.
///
#[doc = example_range!()]
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
#[doc = example_range!()]
///
/// # Panics
///
/// Requires that the given pointer range is in fact contained within the `bytes`, otherwise this
/// function will panic.
///
/// ```should_panic
/// # use bytes::{BytesMut, Bytes};
/// use tcio::{range_of, slice_of_bytes};
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

    assert!(
        sub_p >= bytes_p,
        "range pointer ({:p}) is smaller than `bytes` pointer ({:p})",
        sub_p as *const u8,
        bytes.as_ptr(),
    );
    assert!(
        sub_p + sub_len <= bytes_p + bytes_len,
        "subset is out of bounds: self = ({:p}, {}), subset = ({:p}, {})",
        bytes.as_ptr(),
        bytes_len,
        sub_p as *const u8,
        sub_len,
    );

    let offset = sub_p.saturating_sub(bytes_p);

    bytes.slice(offset..offset + sub_len)
}

