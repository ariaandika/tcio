//! Provide utilities for formatting.

/// [`Debug`] and [`Display`] implementation of `[u8]` that print ASCII graphic character.
///
/// # Examples
///
/// ```
/// use tcio::fmt::lossy;
///
/// let mut bytes = Vec::from(&b"\r\nContent-Type"[..]);
/// bytes.push(0x12);
///
/// assert_eq!(&format!("{}", lossy(&bytes)), &r#"\r\nContent-Type\x12"#[..]);
/// assert_eq!(&format!("{:?}", lossy(&bytes)), &r#"b"\r\nContent-Type\x12""#[..]);
/// ```
///
/// [`Debug`]: std::fmt::Debug
/// [`Display`]: std::fmt::Display
#[inline]
pub fn lossy<B: AsRef<[u8]>>(buf: &B) -> impl std::fmt::Debug + std::fmt::Display {
    std::fmt::from_fn(|f|{
        for &b in buf.as_ref() {
            if b == b'\r' {
                f.write_str("\\r")?;
            } else if b == b'\n' {
                f.write_str("\\n")?;
            } else if b.is_ascii_graphic() || b.is_ascii_whitespace() {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:x}")?;
            }
        }
        Ok(())
    })
}
