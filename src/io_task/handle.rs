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

/// A clonable stateless [`IoTask`] handle.
///
/// This handle is "stateless" in a sense that all operations only requires shared reference, which
/// returns the statefull [`Future`].
///
/// See [crate level docs][super] for more details.
#[derive(Debug, Clone)]
pub struct IoHandle {
    tx: TaskTx,
}

impl IoHandle {
    /// Create new [`IoTask`] with [`IoHandle`] as the handle.
    #[inline]
    pub fn new<IO>(io: IO) -> (IoHandle, IoTask<IO>)
    where
        IO: AsyncIoRead + AsyncIoWrite,
    {
        let (tx, task) = IoTask::new(io);
        (Self { tx }, task)
    }

    // ===== Public =====

    /// Returns `true` if IO task is already closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Read bytes from the underlying IO.
    #[inline]
    pub fn read(&self) -> Read {
        self.read_inner(None)
    }

    /// Read exact size bytes from the underlying IO.
    #[inline]
    pub fn read_exact(&self, len: usize) -> Read {
        self.read_inner(Some(len))
    }

    /// Write bytes to the underlying io.
    #[inline]
    pub fn write(&self, bytes: Bytes) {
        let _ = self.tx.send(TaskTxMessage::Write { bytes });
    }

    /// Wait for all write operation complete.
    #[inline]
    pub fn sync(&self) -> Sync {
        let (tx, rx) = channel();
        match self.tx.send(TaskTxMessage::Sync { tx }) {
            Ok(()) => Sync { repr: Repr::Ok(rx) },
            Err(_) => Sync::closed(),
        }
    }

    // ===== Inner =====

    fn read_inner(&self, cap: Option<usize>) -> Read {
        let (tx, rx) = channel();
        match self.tx.send(TaskTxMessage::Read { cap, tx }) {
            Ok(()) => Read { repr: Repr::Ok(rx) },
            Err(_) => Read::closed(),
        }
    }
}

// ===== Read Future =====

/// Future returned from [`read`][IoHandle::read].
pub struct Read {
    repr: Repr<TaskReadRx>,
}

enum Repr<T> {
    Ok(T),
    Err(Option<io::Error>),
}

impl Read {
    fn closed() -> Self {
        Self {
            repr: Repr::Err(Some(io::ErrorKind::ConnectionAborted.into())),
        }
    }
}

impl Future for Read {
    type Output = io::Result<BytesMut>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context) -> Poll<Self::Output> {
        let result = match &mut self.repr {
            Repr::Ok(rx) => match ready!(Pin::new(rx).poll(cx)) {
                Ok(TaskReadMessage::Data(data)) => Ok(data),
                Ok(TaskReadMessage::Err(err)) => Err(err),
                Err(_) => Err(io::ErrorKind::ConnectionAborted.into()),
            },
            Repr::Err(err) => Err(err.take().unwrap()),
        };

        Poll::Ready(result)
    }
}

impl std::fmt::Debug for Read {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Read").finish_non_exhaustive()
    }
}

// ===== Sync Future =====

/// Future returned from [`sync`][IoHandle::sync].
pub struct Sync {
    repr: Repr<TaskSyncRx>,
}

impl Sync {
    fn closed() -> Self {
        Self {
            repr: Repr::Err(Some(io::ErrorKind::ConnectionAborted.into())),
        }
    }
}

impl Future for Sync {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context) -> Poll<Self::Output> {
        let result = match &mut self.repr {
            Repr::Ok(rx) => match ready!(Pin::new(rx).poll(cx)) {
                Ok(TaskSyncMessage::Pending) => return Poll::Pending,
                Ok(TaskSyncMessage::Ok) => Ok(()),
                Ok(TaskSyncMessage::Err(err)) => Err(err),
                Err(_) => Err(io::ErrorKind::ConnectionAborted.into()),
            },
            Repr::Err(err) => Err(err.take().unwrap()),
        };

        Poll::Ready(result)
    }
}

impl std::fmt::Debug for Sync {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Sync").finish_non_exhaustive()
    }
}

