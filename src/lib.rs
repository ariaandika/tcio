//! Collection of utility types.
#![warn(missing_docs, missing_debug_implementations)]

mod slice;
mod bytestr;
mod either;
mod either_map;

pub mod futures;
pub mod io;

#[cfg(feature = "tokio")]
pub mod tokio;

pub use slice::{range_of, slice_of_bytes};
pub use bytestr::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;

