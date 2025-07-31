//! Raw bytes utilities.
mod shared;

mod bytes_ref;
mod bytes_mut;
mod cursor;
mod range;

use shared::{Shared, Data, DataMut};

pub use bytes_ref::Bytes;
pub use bytes_mut::BytesMut;
pub use cursor::Cursor;
pub use range::{range_of, slice_of, slice_of_bytes, slice_of_bytes_mut};
