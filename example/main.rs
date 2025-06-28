use std::{
    future::poll_fn,
    io::{self, Write},
    net::TcpStream,
    task::{Poll, ready},
    thread::sleep,
    time::Duration,
};
use tcio::io::{AsyncBufRead, BufCursor, BufReader};
use tokio::{net::TcpListener, runtime::Runtime};

fn main() -> io::Result<()> {
    std::thread::spawn(|| client().unwrap());

    Runtime::new().unwrap().block_on(async {
        let tcp = TcpListener::bind("0.0.0.0:3000").await?;

        loop {
            let Ok((io, _)) = tcp.accept().await else {
                continue;
            };

            let mut io = BufReader::new(io);

            poll_fn(|cx| parse(&mut io, cx)).await?;

            assert!(io.chunk().is_empty());

            break Ok(());
        }
    })
}

fn parse<B: AsyncBufRead>(io: B, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
    let mut cursor = BufCursor::new(io);

    let chunk = ready!(cursor.poll_get(3, cx)?);

    assert_eq!(chunk, b"Foo");

    let chunk = ready!(cursor.poll_get(3, cx)?);

    assert_eq!(chunk, b"Bar");

    cursor.commit();

    Poll::Ready(Ok(()))
}

fn client() -> Result<(), io::Error> {
    sleep(Duration::from_millis(10));

    let mut io = TcpStream::connect("0.0.0.0:3000")?;

    io.write_all(b"Foo")?;

    io.write_all(b"Bar")?;

    Ok(())
}
