# Alternative Bytes Representation

Initially, `Bytes` API is a copy of the `bytes` crate with different internal
behavior. Therefore, it follows the vtable strategy.

But because this API does not intent to cover any use cases, this new
representation attempts to eliminate the vtable.

In shorts, this simmilar to how `BytesMut` works, without the `capacity` field.
This can only be stored if the `length` and `capacity` is equal. Otherwise, the
optimization such lazy state allocation will not be used.

