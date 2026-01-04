//! Provide utilities that represent either types which implement the same trait.
//!
//! [`Either`] and [`EitherMap`] have different behavior on some trait implementation. Refer to its
//! struct level docs for more detail.
mod either;
mod either_map;
pub use either::Either;
pub use either_map::EitherMap;
