//! Raw bytes utilities.
#![allow(missing_docs, reason = "wip")]
mod shared;

mod buf;
mod buf_mut;
mod bytes;
mod bytes_mut;
mod bytestr;
mod cursor;

use shared::{Shared, Data, DataMut};

pub use buf::Buf;
pub use buf_mut::BufMut;
pub use bytes::Bytes;
pub use bytes_mut::BytesMut;
pub use bytestr::ByteStr;
pub use cursor::Cursor;
