//! Integration with [`tokio`] crate.
mod poll;
mod stream;

pub use poll::{poll_read, poll_write_all};
pub use stream::IoStream;

