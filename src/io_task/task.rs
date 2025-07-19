use bytes::{Bytes, BytesMut};
use std::{collections::VecDeque, io, mem::take, pin::Pin, task::Poll};
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot::{Receiver, Sender},
};

use crate::io::{AsyncIoRead, AsyncIoWrite};

pub(crate) type TaskTx = UnboundedSender<TaskTxMessage>;
pub(crate) type TaskRx = UnboundedReceiver<TaskTxMessage>;
pub(crate) type TaskReadRx = Receiver<TaskReadMessage>;
pub(crate) type TaskSyncTx = Sender<TaskSyncMessage>;
pub(crate) type TaskSyncRx = Receiver<TaskSyncMessage>;
type HandleTx = Sender<TaskReadMessage>;

struct ReadTask {
    cap: Option<usize>,
    tx: HandleTx,
}

impl ReadTask {
    fn send(self, data: TaskReadMessage) {
        let _ = self.tx.send(data);
    }
}

struct WriteTask {
    bytes: Bytes,
}

pub enum TaskTxMessage {
    /// Read from io with optional exact size.
    ///
    /// When ready, [`Data`][TaskReadMessage::Data] message will be send to `tx`.
    Read {
        cap: Option<usize>,
        tx: HandleTx,
    },
    /// Write given bytes to io.
    Write {
        bytes: Bytes,
    },
    /// Request `writing` status, a [`TaskSyncMessage`] will be send immediately.
    Sync {
        tx: TaskSyncTx,
    },
}

/// Result for [`Read`][TaskTxMessage::Read] request.
pub enum TaskReadMessage {
    /// Successfull read.
    Data(BytesMut),
    /// An error occured when reading.
    Err(io::Error),
}

/// Status for [`Sync`][TaskTxMessage::Sync] request.
pub enum TaskSyncMessage {
    /// There is still write operation pending.
    Pending,
    /// All write operations completed successfully.
    Ok,
    /// An error occured on the most recent write operation.
    Err(io::Error),
}

// ===== IoTask =====

/// A future to drive the concurent io operation.
///
/// See [crate level docs][super] for more details.
pub struct IoTask<IO> {
    rx: TaskRx,
    io: IO,
    buffer: BytesMut,
    read_queue: VecDeque<ReadTask>,
    write_queue: VecDeque<WriteTask>,
    write_err: Option<io::Error>,
}

impl<IO> Unpin for IoTask<IO> {}

impl<IO> IoTask<IO> {
    pub(crate) fn new(io: IO) -> (TaskTx, Self) {
        let (tx, rx) = unbounded_channel();
        let me = Self {
            rx,
            io,
            buffer: BytesMut::with_capacity(0x0400),
            read_queue: VecDeque::new(),
            write_queue: VecDeque::new(),
            write_err: None,
        };
        (tx, me)
    }

    // ===== Helper =====

    fn terminate(&mut self) {
        self.write_err = Some(io::ErrorKind::ConnectionAborted.into());
        for task in take(&mut self.read_queue) {
            task.send(TaskReadMessage::Err(io::ErrorKind::ConnectionAborted.into()));
        }
    }

    fn is_terminating(&self) -> bool {
        matches!(&self.write_err, Some(err) if err.kind() == io::ErrorKind::ConnectionAborted)
    }

    fn can_terminate(&self) -> bool {
        self.is_terminating() && self.write_queue.is_empty()
    }

    fn send_reader(&mut self, data: BytesMut) {
        if let Some(task) = self.read_queue.pop_front() {
            task.send(TaskReadMessage::Data(data));
        }
    }

    fn send_reader_err(&mut self, err: io::Error) {
        if let Some(task) = self.read_queue.pop_front() {
            task.send(TaskReadMessage::Err(err));
        }
    }

    // ===== Operations =====

    fn poll_message(&mut self, cx: &mut std::task::Context) {
        if self.is_terminating() {
            return;
        }

        let msg = match self.rx.poll_recv(cx) {
            Poll::Ready(Some(msg)) => msg,
            Poll::Ready(None) => {
                self.terminate();
                return;
            }
            Poll::Pending => return,
        };

        match msg {
            TaskTxMessage::Read { cap, tx } => self.read_queue.push_back(ReadTask { cap, tx }),
            TaskTxMessage::Write { bytes } => self.write_queue.push_back(WriteTask { bytes }),
            TaskTxMessage::Sync { tx } => self.handle_sync(tx),
        }

        self.poll_message(cx)
    }

    fn handle_sync(&mut self, tx: Sender<TaskSyncMessage>) {
        let _ = tx.send(match (self.write_err.take(), self.write_queue.is_empty()) {
            (None, true) => TaskSyncMessage::Ok,
            (None, false) => TaskSyncMessage::Pending,
            (Some(err), _) => TaskSyncMessage::Err(err),
        });
    }
}

impl<IO> IoTask<IO>
where
    IO: AsyncIoRead + AsyncIoWrite,
{
    fn poll_read(&mut self, cx: &mut std::task::Context) {
        if self.is_terminating() {
            return;
        }

        self.handle_buffer();

        if self.read_queue.is_empty() {
            return;
        }

        // io call

        if self.buffer.capacity() < 0x0100 && self.buffer.len() < 0x400 {
            self.buffer.reserve(0x0400 - self.buffer.len());
        }

        let Poll::Ready(result) = self.io.poll_read_buf(&mut self.buffer, cx) else {
            return;
        };

        match result {
            Ok(0) => self.terminate(),
            Ok(_) => {
                self.handle_buffer();
                self.poll_read(cx);
            },
            Err(err) => self.send_reader_err(err),
        }
    }

    /// Check is current buffer is enough to send back to handle.
    fn handle_buffer(&mut self) {
        let Some(task) = self.read_queue.front_mut() else {
            return;
        };

        match task.cap {
            None => if !self.buffer.is_empty() {
                let data = self.buffer.split();
                self.send_reader(data);
            },
            Some(remaining) => if self.buffer.len() >= remaining {
                let data = self.buffer.split_to(remaining);
                self.send_reader(data);
            },
        }
    }

    fn poll_write(&mut self, cx: &mut std::task::Context) {
        let Some(task) = self.write_queue.front_mut() else {
            return;
        };

        let Poll::Ready(result) = self.io.poll_write_all_buf(&mut task.bytes, cx) else {
            return;
        };

        if let Err(err) = result {
            self.write_err = Some(err);
        }

        self.write_queue.pop_front();
        self.poll_write(cx);
    }

    fn try_poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context) -> Poll<()> {
        let me = self.as_mut().get_mut();

        me.poll_message(cx);
        me.poll_read(cx);
        me.poll_write(cx);

        Poll::Pending
    }
}

// ===== traits =====

impl<IO> Future for IoTask<IO>
where
    IO: AsyncIoRead + AsyncIoWrite,
{
    type Output = ();

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context) -> Poll<Self::Output> {
        match self.as_mut().try_poll(cx) {
            Poll::Ready(()) => Poll::Ready(()),
            Poll::Pending => {
                if self.can_terminate() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl<IO> std::fmt::Debug for IoTask<IO> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("IoTask").finish_non_exhaustive()
    }
}
