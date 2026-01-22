//! Provide utilities for working with bytes.
//!
//! Overview:
//!
//! - [`Buf`] represent a readable in memory buffer.
//! - [`BufMut`] represent a writable in memory buffer.
//! - [`Bytes`] is a reference counted shared memory buffer.
//! - [`ByteStr`] is a `Bytes` that contains valid UTF-8.
//! - [`BytesMut`] is a splitable in memory buffer.
mod shared;

mod buf;
mod buf_mut;
mod bytes;
mod bytes_mut;
mod bytestr;
mod uninit_slice;
mod take;
mod chain;

pub use buf::Buf;
pub use buf_mut::BufMut;
pub use bytes::Bytes;
pub use bytes_mut::BytesMut;
pub use bytestr::ByteStr;
pub use uninit_slice::UninitSlice;
pub use take::Take;
pub use chain::Chain;

pub mod error {
    //! Error types.
    pub use super::bytestr::FromUtf8Error;
}

#[cfg(test)]
mod test;
