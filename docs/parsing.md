# Data Format

Multiple kind of data format and what parsing api can be used.

## Message

Requires small buffer to contains the full information.

A single message can be stored entirely in memory.

Stateless parsing, when more read is required, retrying from the beginning is
fine.

e.g: websocket frame, postgres protocol message.

## Streaming Messages

Requires large buffer to contains the full information.

The data can be read as separate messages, but requires all the messages to
understand the full information.

A single message may contain very large buffer which is not sufficient to be
stored in memory.

Stateful parsing, when more read is required, retry from the last
message boundary.

Complex implementation, maintaining the state of previous parsing after more
data read.

e.g: http, multipart, chunked encoding

## Buffered

Requires small to medium buffer to contains the full information.

A single message can be stored entirely in memory.

One time parsing, when the buffer is not sufficient, an error is
returned.

e.g: json

# Asynchronous Parsing

Stateless parsing

```rust
fn poll_parse<IO>(io: IO, cx: &mut Context) -> Poll<io::Result<()>> {
    let chunk = ready!(io.read(..4)?);

    // no checkpoint, when Poll::Pending, retry from the beginning

    // ...
}
```

Stateful parsing

```rust
impl Parser {
    fn poll_parse<IO>(&mut self, io: IO, cx: &mut Context) -> Poll<io::Result<()>> {
        match self.phase {
            Phase::Header => {
                let chunk = ready!(io.read(..4)?);

                // when Poll::Pending, restart from the last `Phase`

                // ...
            },
            // ...
        }
    }
}
```

# IO Stream API

The following is an example of stateless parsing. `IO` is a buffered io stream.
Meaning it can read more data from the underlying io, as well as store the
buffer internally.

```rust
fn poll_parse<IO>(io: IO, cx: &mut Context) -> Poll<io::Result<()>> {
    let mut cursor = io.cursor();

    let chunk = ready!(cursor.poll_read(4, cx)?);

    // ...

    let chunk2 = ready!(cursor.poll_read(6, cx)?);

    assert_ne!(chunk, chunk2);

    // ...

    cursor.commit();
    Poll::Ready(Ok(()))
}
```

`poll_parse` claim a guarantee:

- if the parsing is incomplete, either by an error or pending, the `io`
  internal buffer will not be advanced.

Parser cannot simply read and advance every single time it requires a chunk,
because an error or pending can occur immediately. Therefore, a `Cursor` can
help with this control flow.

A `Cursor` is an `IO` wrapper and have the same API as `IO`. But it track the
read buffer internally, not advancing the underlying `IO`. When parsing is
complete, calling `Cursor::commit` will actually advance the `IO` by the amount
of internally tracked read buffer.

