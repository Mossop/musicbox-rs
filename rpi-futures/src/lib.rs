//! A set of helpful utilities to use the Raspberry Pi hardware using futures
//! and streams rather than callbacks.
//!
//! Currently this is a set of wrappers around [`rppal`](https://crates.io/crates/rppal)'s
//! GPIO inputs.
#![warn(missing_docs)]

pub mod gpio;
