//! A concurrent IO operation.
//!
//! [`IoTask`] is a task which will drive an IO object to provide concurent operation. To interact
//! with the task, callers will be provided with a `handle`. There is multiple kind of `handle` to
//! provide flexibility.
//!
//! # Handle
//!
//! [`IoHandle`] is a stateless handle, all method returns the statefull [`Future`]. Good in
//! userspace, where `.await` can be just used.
//!
//! [`IoHandle`] is a statefull handle, all method is polling based.

#![allow(missing_debug_implementations, missing_docs, reason = "wip")]

mod task;
mod handle;
mod poll;

pub(crate) use task::{TaskTxMessage, TaskReadMessage, TaskSyncMessage};

pub use task::IoTask;
pub use handle::{IoHandle, Read, Sync};
pub use poll::IoPoll;

