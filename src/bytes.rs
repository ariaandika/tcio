//! Raw bytes utilities.
mod cursor;
mod range;

pub use cursor::Cursor;
pub use range::{range_of, slice_of, slice_of_bytes, slice_of_bytes_mut};
