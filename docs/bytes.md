# Bytes

```rust
use tcio::bytes::Bytes;
```

## Overview

Immutable reference counted raw bytes.

Can be imagined as `Arc<[u8]>` with additional API and optimization.

Usually, `Bytes` constructed by splitting and "freezing" `BytesMut`.

Or if required in parameter, `Bytes` can be constructed from any heap allocated
bytes.

## As a view of contiguous bytes

`Bytes` can represent only the subset of the actual allocated bytes.

In contrast with `Vec<u8>`, if user wants to remove the leading bytes, the
bytes requires to be copied and reallocated. But in `Bytes`, none of this
happens.

`Bytes::{advance, truncate, truncate_off}` provide an API to control the view
bounds of the `Bytes`.

Additionally, `Bytes` does not introduce double pointer indirection, in
contrast with `Arc<[u8]>`. The pointer of the bytes stored directly in the
struct.

## Reference Counted

`Bytes::clone` does not copy nor allocate the bytes, instead it just create new
view. Which means, cloning `Bytes` is cheap, `O(1)` operation.

The lifetime of the heap allocation is managed with atomic reference counter.
Thus, cloning only increments the counter, and when dropped, counter is
decremented. When the counter is 0, the heap memory is deallocated.

## Splitting

Combine view structure and reference counted, `Bytes` can be used to cheaply
split contiguous bytes into individual data.

Splitting bytes is heavily used in protocol parsing.

## Use Cases

`Bytes` is intended for storing parsed information from raw bytes.

For example, HTTP request message have properties that is an arbitrary length
bytes. **Splitting** the original buffer into individual properties requires
multiple copies and allocations, 1 for URI, 2 for each headers, etc.

With `Bytes`, all properties use single allocation.

```
(heap)  : "GET /users/all HTTP/1.1\r\nContent-type: text/html\r\n\r\n"
message : [----------------------------------------------------------]
uri     :     [----------]
h1_name :                            [------------]
h1_value:                                          [---------]
```

This can be done by the facts that most protocol parsing result is immutable.

## Optimization

Reference counted data usually perform two allocation, the reference counter
state, and the actual data. `Bytes` have an optimization where it does not
allocate the reference counter state if it does not need to. The actual data
pointer is stored directly in the struct.

## Backgrounds

This API is port from the popular `bytes` crate from the tokio team. The
difference is that this version does not intent to cover all use cases,
therefore it have different internal representation.

