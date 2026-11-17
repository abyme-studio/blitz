//! The wire format of one React commit, and its application to a Blitz document.
//!
//! A commit is two buffers: `ops`, little-endian `u32` words, and `strings`, a table of UTF-8
//! strings each prefixed by its byte length as a little-endian `u32`. An op is its code followed by
//! its arguments; a string argument is an index into the commit's table. Node ids are `u32`s JS
//! chooses, so a create needs no answer; [`Tree`] maps them to Blitz's node ids.
//!
//! This crate is the one owner of the format: `react-blitz/js`'s encoder writes it, and the golden
//! files in `react-blitz/tests/golden/` hold both to it.

mod apply;
mod decode;
mod ops;

pub use apply::{ApplyError, Tree};
pub use decode::{DecodeError, decode};
pub use ops::{Namespace, Op, OpCode};
