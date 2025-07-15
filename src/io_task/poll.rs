use bytes::{Bytes, BytesMut};
use std::{
    io,
    pin::Pin,
    task::{Poll, ready},
};
use tokio::sync::oneshot::channel;

use crate::io::{AsyncIoRead, AsyncIoWrite};

use super::{
    IoTask, TaskReadMessage, TaskSyncMessage, TaskTxMessage,
    task::{TaskReadRx, TaskSyncRx, TaskTx},
};

macro_rules! poll_err {
    ($e:expr) => {
        Poll::Ready(Err(io::Error::other($e)))
    };
}

/// A statefull [`IoTask`] handle.
///
/// This handle is "statefull" in a sense that all operations requires mutable reference, and
/// provide poll based operation.
///
/// Note that all `poll_*` operation is not cancel safe. If `poll_read` called returns pending, and
/// then user call `poll_sync`, it will returns an error.
///
/// See [crate level docs][super] for more details.
pub struct IoPoll {
    ops: Option<Operation>,
    tx: TaskTx,
}

enum Operation {
    Read(TaskReadRx),
    Sync(TaskSyncRx),
}

impl IoPoll {
    pub(crate) fn from_spawned(tx: TaskTx) -> Self {
        Self { ops: None, tx }
    }

    /// Create new [`IoTask`] with [`IoPoll`] as the handle.
    #[inline]
    pub fn new<IO>(io: IO) -> (IoPoll, IoTask<IO>)
    where
        IO: AsyncIoRead + AsyncIoWrite,
    {
        let (tx, task) = IoTask::new(io);
        (Self { ops: None, tx, }, task)
    }

    // ===== Public =====

    /// Returns `true` if IO task is already closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Poll for read bytes from underlying IO.
    #[inline]
    pub fn poll_read(&mut self, cx: &mut std::task::Context) -> Poll<io::Result<BytesMut>> {
        self.poll_read_inner(None, cx)
    }

    /// Poll for read exact size bytes from underlying IO.
    #[inline]
    pub fn poll_read_exact(&mut self, len: usize, cx: &mut std::task::Context) -> Poll<io::Result<BytesMut>> {
        self.poll_read_inner(Some(len), cx)
    }

    fn poll_read_inner(&mut self, cap: Option<usize>, cx: &mut std::task::Context) -> Poll<io::Result<BytesMut>> {
        use Operation::*;

        match &mut self.ops {
            Some(Read(rx)) => {
                let result = match ready!(Pin::new(rx).poll(cx)) {
                    Ok(TaskReadMessage::Data(ok)) => Ok(ok),
                    Ok(TaskReadMessage::Err(err)) => Err(err),
                    Err(_) => return poll_err!("`IoTask` is already closed")
                };
                self.ops.take();
                Poll::Ready(result)
            },
            Some(Sync(_)) => poll_err!("`IoPoll::poll_sync` is pending"),
            None => {
                let (tx, rx) = channel();
                if self.tx.send(TaskTxMessage::Read { cap, tx }).is_err() {
                    return poll_err!("`IoTask` is already closed");
                }
                self.ops = Some(Read(rx));
                self.poll_read_inner(cap, cx)
            },
        }
    }

    /// Write bytes to the underlying io.
    #[inline]
    pub fn write(&self, bytes: Bytes) {
        let _ = self.tx.send(TaskTxMessage::Write { bytes });
    }

    /// Poll for all write operations completions.
    pub fn poll_sync(&mut self, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
        use Operation::*;

        match &mut self.ops {
            Some(Read(_)) => poll_err!("`IoPoll::poll_read` is pending"),
            Some(Sync(rx)) => {
                let result = match ready!(Pin::new(rx).poll(cx)) {
                    Ok(TaskSyncMessage::Pending) => return Poll::Pending,
                    Ok(TaskSyncMessage::Ok) => Ok(()),
                    Ok(TaskSyncMessage::Err(err)) => Err(err),
                    Err(_) => return poll_err!("`IoTask` is already closed")
                };
                self.ops.take();
                Poll::Ready(result)
            },
            None => {
                let (tx, rx) = channel();
                if self.tx.send(TaskTxMessage::Sync { tx }).is_err() {
                    return poll_err!("`IoTask` is already closed");
                }
                self.ops = Some(Sync(rx));
                self.poll_sync(cx)
            },
        }
    }

    /// Convert into shared handle [`IoHandle`][super::IoHandle].
    #[inline]
    pub fn into_handle(self) -> super::IoHandle {
        super::IoHandle::from_spawned(self.tx)
    }
}

impl std::fmt::Debug for IoPoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IoPoll")
            .field("state", match &self.ops {
                Some(Operation::Read(_)) => &"Reading",
                Some(Operation::Sync(_)) => &"Syncing",
                None => &"Idle",
            })
            .finish()
    }
}

