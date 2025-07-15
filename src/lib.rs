//! # Utilities
#![warn(missing_docs, missing_debug_implementations)]

mod bytestr;
mod either;
mod either_map;

pub mod slice;
pub mod futures;
pub mod io;
pub mod fmt;
pub mod sync;

#[cfg(feature = "tokio")]
pub mod tokio;
#[cfg(feature = "tokio")]
pub mod io_task;

// ===== Re-exports =====

pub use bytestr::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;
