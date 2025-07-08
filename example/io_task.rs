//! Example for creating task based io operation.
use std::{future::poll_fn, io};
use tcio::io::AsyncIoWrite;
use tokio::{
    net::{TcpListener, TcpStream},
    runtime::Runtime,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Runtime::new().unwrap().block_on(async {
        let tcp = TcpListener::bind("0.0.0.0:3000").await?;

        tokio::spawn(sender());

        let (io, _) = tcp.accept().await?;

        let mut handle = task::IoHandle::new_spawn(io);

        let data = poll_fn(|cx| handle.poll_read(cx)).await?;
        assert_eq!(data.as_ref(), b"foo");

        Ok(())
    })
}

async fn sender() -> Result<(), io::Error> {
    let io = TcpStream::connect("0.0.0.0:3000").await?;

    poll_fn(|cx| io.poll_write_all_buf(&mut &b"foo"[..], cx)).await?;

    Ok(())
}

mod task {
    use bytes::{Bytes, BytesMut};
    use std::{
        io,
        pin::Pin,
        task::{Poll, ready},
    };
    use tcio::tokio::{poll_read, poll_write_all};
    use tokio::{
        io::{AsyncRead, AsyncWrite},
        sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    };

    type Tx = UnboundedSender<Message>;
    type Rx = UnboundedReceiver<Message>;

    /// Handle for operating with task based shared io operaion.
    #[derive(Debug)]
    pub struct IoHandle {
        send: bool,
        tx: Tx,
        rx: Rx,
    }

    impl IoHandle {
        /// Create new [`IoHandle`].
        pub fn new<IO>(io: IO) -> (Self, IoTask<IO>) {
            let (s1, r1) = unbounded_channel();
            let (s2, r2) = unbounded_channel();

            let me = Self {
                send: false,
                tx: s1,
                rx: r2
            };

            let task = IoTask {
                io,
                tx: s2,
                rx: r1,
                phase: Phase::Idle,
                buffer: BytesMut::with_capacity(512),
            };

            (me, task)
        }

        /// Create new [`IoHandle`].
        #[inline]
        pub fn new_spawn<IO>(io: IO) -> IoHandle
        where
            IO: AsyncRead + AsyncWrite + Send + 'static,
        {
            let (me, task) = Self::new(io);
            tokio::spawn(task);
            me
        }

        pub fn poll_read(&mut self, cx: &mut std::task::Context) -> Poll<Result<Bytes, Error>> {
            if !self.send {
                if self.tx.send(Message::Read).is_err() {
                    return Poll::Ready(Err(ErrorKind::ChannelClosed.into()));
                }
                self.send = true;
            }
            let result = match ready!(self.rx.poll_recv(cx)) {
                Some(Message::Data(Ok(data))) => Ok(data),
                Some(Message::Data(Err(err))) => Err(err.into()),
                Some(_) => return self.poll_read(cx),
                None => Err(ErrorKind::ChannelClosed.into()),
            };
            Poll::Ready(result)
        }

        #[allow(unused, reason = "example")]
        pub fn read(&mut self) -> impl Future<Output = Result<Bytes, Error>> {
            std::future::poll_fn(|cx| self.poll_read(cx))
        }
    }

    /// Future for excuting the shared io operation.
    #[derive(Debug)]
    pub struct IoTask<IO> {
        io: IO,
        tx: Tx,
        rx: Rx,
        phase: Phase,
        buffer: BytesMut,
    }

    #[derive(Debug)]
    enum Phase {
        Idle,
        Read,
        Write(Bytes),
    }

    struct IoTaskProject<'a, IO> {
        io: Pin<&'a mut IO>,
        tx: &'a mut Tx,
        rx: &'a mut Rx,
        phase: &'a mut Phase,
        read_buf: &'a mut BytesMut,
    }

    enum Message {
        Data(io::Result<Bytes>),
        Read,
    }

    impl<IO> IoTask<IO> {
        fn project(self: Pin<&mut Self>) -> IoTaskProject<'_, IO> {
            // SAFETY: self is pinned
            // no `Drop`, nor manual `Unpin` implementation.
            unsafe {
                let me = self.get_unchecked_mut();
                IoTaskProject {
                    io: Pin::new_unchecked(&mut me.io),
                    tx: &mut me.tx,
                    rx: &mut me.rx,
                    phase: &mut me.phase,
                    read_buf: &mut me.buffer,
                }
            }
        }
    }

    impl<IO> Future for IoTask<IO>
    where
        IO: AsyncRead + AsyncWrite,
    {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context) -> Poll<Self::Output> {
            let mut me = self.as_mut().project();

            loop {
                match me.phase {
                    Phase::Idle => {
                        let Some(msg) = ready!(me.rx.poll_recv(cx)) else {
                            return Poll::Ready(());
                        };

                        match msg {
                            Message::Read => *me.phase = Phase::Read,
                            Message::Data(Ok(data)) => *me.phase = Phase::Write(data),
                            Message::Data(Err(_)) => {}
                        }
                    }
                    Phase::Read => {
                        let result = ready!(poll_read(&mut me.read_buf, &mut me.io, cx));
                        match result {
                            Ok(read) => {
                                let read = me.read_buf.split_to(read);
                                let _ = me.tx.send(Message::Data(Ok(read.freeze())));
                            }
                            Err(err) => {
                                let _ = me.tx.send(Message::Data(Err(err)));
                            }
                        }
                        *me.phase = Phase::Idle;
                    }
                    Phase::Write(data) => {
                        let result = ready!(poll_write_all(data, &mut me.io, cx));
                        if let Err(err) = result {
                            let _ = me.tx.send(Message::Data(Err(err)));
                        }
                        *me.phase = Phase::Idle;
                    }
                }
            }
        }
    }

    // ===== Error =====

    /// Error which can occur during [`IoHandle`] operations.
    #[derive(Debug)]
    pub struct Error {
        kind: ErrorKind,
    }

    #[derive(Debug)]
    enum ErrorKind {
        Io(io::Error),
        ChannelClosed,
    }

    impl From<io::Error> for Error{
        fn from(v: io::Error) -> Self {
            Self { kind: ErrorKind::Io(v) }
        }
    }

    impl From<ErrorKind> for Error {
        fn from(kind: ErrorKind) -> Self {
            Self { kind }
        }
    }

    impl std::error::Error for Error {}
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match &self.kind {
                ErrorKind::Io(err) => err.fmt(f),
                ErrorKind::ChannelClosed => "channel closed".fmt(f),
            }
        }
    }
}
