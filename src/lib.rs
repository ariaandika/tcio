//! Collection of utility types.
#![warn(missing_docs, missing_debug_implementations)]

pub mod slice;
mod bytestr;
mod either;
mod either_map;

pub mod futures;
pub mod io;
pub mod fmt;

#[cfg(feature = "tokio")]
pub mod tokio;

pub use bytestr::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;

