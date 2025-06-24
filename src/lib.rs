//! Collection of small utility types.
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_inline_in_public_items)]

mod bytestr;
mod either;
mod either_map;

pub mod futures;

pub use bytestr::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;

