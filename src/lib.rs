//! # TcIO.
//!
//! Collection of utilities for working with async io and raw bytes.
#![warn(missing_docs, missing_debug_implementations)]
#![allow(clippy::module_inception)]

mod macros;

pub mod bytes;
pub mod either;
pub mod fmt;
pub mod futures;
pub mod io;
pub mod num;

#[cfg(feature = "tokio")]
pub mod tokio;
