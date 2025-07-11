# Changelog

## v0.1.4 (July 11 2025)

### Added

- add `pop_front` and `pop_chunk_front` method for `Cursor`
- implement `Iterator` and `ExactSizeIterator` for `Cursor`
- add `step_back` method for Cursor
- add `step` and `original` method for Cursor
- add `poll_read_fn` function
- add `poll_read` and `poll_write_all` function
- add `Recall` struct
- add `slice_of_bytes_mut` function

### Fixed

- fix panic when creating `Cursor` with empty string

## v0.1.3

### Added

- add `slice_of` funtion
- add formatting utility
- add `as_bytes`, `buffer`, and `buffer_mut` method of BufReader
- add `inner` and `inner_mut` method of BufReader
- add `Cursor` struct

## v0.1.2

### Added

- add `range_of` and `slice_of_bytes` function

## v0.1.1

### Added

- add `try_mut` method
- forward `is_unique` and `clear` from `Bytes` in `ByteStr`
- add `AsyncBufRead` trait
- add `BufCursor` struct

### Changed

- split `AsyncIo` into `AsyncIoRead` and `AsyncIoWrite`

## v0.1.0
