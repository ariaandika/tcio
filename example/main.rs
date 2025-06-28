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

            poll_fn(|cx| poll_parse(&mut io, cx)).await?;

            assert!(io.chunk().is_empty());

            break Ok(());
        }
    })
}

fn poll_parse<B: AsyncBufRead>(io: B, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
    let mut cursor = BufCursor::new(io);

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Foo");

    ready!(poll_parse_uncommit(&mut cursor, cx)?);

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Bar");

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Baz");

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Buf");

    cursor.commit();

    Poll::Ready(Ok(()))
}

fn poll_parse_uncommit<B: AsyncBufRead>(io: B, cx: &mut std::task::Context) -> Poll<io::Result<()>> {
    let mut cursor = BufCursor::new(io);

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Bar");

    let chunk = ready!(cursor.poll_get(3, cx)?);
    assert_eq!(chunk, b"Baz");

    Poll::Ready(Ok(()))
}

fn client() -> Result<(), io::Error> {
    sleep(Duration::from_millis(10));

    let mut io = TcpStream::connect("0.0.0.0:3000")?;

    io.write_all(b"Foo")?;
    sleep(Duration::from_millis(10));
    io.write_all(b"Bar")?;
    sleep(Duration::from_millis(10));
    io.write_all(b"Baz")?;
    sleep(Duration::from_millis(10));
    io.write_all(b"Buf")?;

    Ok(())
}
