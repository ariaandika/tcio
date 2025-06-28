//! Collection of utility types.
#![warn(missing_docs, missing_debug_implementations)]

mod bytestr;
mod either;
mod either_map;

pub mod futures;
pub mod io;

#[cfg(feature = "tokio")]
pub mod tokio;

pub use bytestr::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;

