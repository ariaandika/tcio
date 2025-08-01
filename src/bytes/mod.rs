//! Raw bytes utilities.
mod shared;

#[allow(clippy::module_inception)]
mod bytes;
mod bytes_mut;
mod bytestr;
mod cursor;
mod range;

use shared::{Shared, Data, DataMut};

pub use bytes::Bytes;
pub use bytes_mut::BytesMut;
pub use bytestr::ByteStr;
pub use cursor::Cursor;
pub use range::{range_of, slice_of, slice_of_bytes, slice_of_bytes_mut};
