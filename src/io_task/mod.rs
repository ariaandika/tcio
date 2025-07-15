//! A concurrent IO operation.
//!
//! [`IoTask`] is a task which will drive an IO object to provide concurent operation. To interact
//! with the task, callers will be provided with a `handle`. There is multiple kind of `handle` to
//! provide flexibility.
//!
//! # Handle
//!
//! [`IoHandle`] is the stateless handle, all method returns the statefull [`Future`]. This allows
//! for reference only operation.

// [`IoClient`] is the statefull handle, all method is in a `poll` form.

#![allow(missing_debug_implementations, missing_docs, reason = "wip")]

mod task;
mod handle;

pub use task::{IoTask, TaskTxMessage, TaskReadMessage, TaskSyncMessage};
pub use handle::{IoHandle, Read};

