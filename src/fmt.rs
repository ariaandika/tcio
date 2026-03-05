//! Provide utilities for formatting.
use std::fmt::{Debug, Display};

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
pub fn lossy<B: AsRef<[u8]>>(buf: &B) -> impl Debug + Display {
    LossyFmt(buf.as_ref())
}

/// Return type of [`lossy`].
struct LossyFmt<'a>(&'a [u8]);

impl Display for LossyFmt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &b in self.0 {
            if b == b'\r' {
                write!(f, "\\r")?;
            } else if b == b'\n' {
                write!(f, "\\n")?;
            } else if b.is_ascii_graphic() || b.is_ascii_whitespace() {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:x}")?;
            }
        }
        Ok(())
    }
}

impl Debug for LossyFmt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "b\"{self}\"")
    }
}

