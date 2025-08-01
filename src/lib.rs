//! # TcIO.
//!
//! An APIs so small that it can be merged together.
//!
//! - [`ByteStr`], a [`Bytes`] that contains a valid UTF-8
//! - [`Either`], represent 2 types that have the same behavior
//! - [`atoi`], parse bytes directly to signed/unsigned integer
//! - [`Cursor`], iterate over bytes without bounds checking
//! - [`Future`] adapters
//!
//! This crate also contains shared async types, such:
//!
//! - [`AsyncIoRead`] and [`AsyncIoWrite`]
//!
//! Other types are exploration that may or may not be persist in future version.
//!
//! [`Bytes`]: bytes::Bytes
//! [`atoi`]: atoi::atoi
//! [`Cursor`]: bytes::Cursor
//! [`AsyncIoRead`]: io::AsyncIoRead
//! [`AsyncIoWrite`]: io::AsyncIoWrite
#![warn(missing_docs, missing_debug_implementations)]
#![allow(clippy::module_inception)]

mod macros;
mod either;
mod either_map;
mod atoi;

pub mod bytes;
pub mod futures;
pub mod io;
pub mod fmt;
pub mod sync;

#[cfg(feature = "tokio")]
pub mod tokio;
#[cfg(feature = "tokio")]
pub mod io_task;

// ===== Re-exports =====

pub use bytes::ByteStr;
pub use either::Either;
pub use either_map::EitherMap;
pub use atoi::{atou, atoi};
