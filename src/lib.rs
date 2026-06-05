//! eWatts Protocol — Library crate
//! This file includes main.rs so both bin and lib targets share the same code.
//! The lib target is used by fuzz targets and external tools.

#![allow(unused_imports)]

// `mod tests` is conditional on cfg(test) - don't include unconditionally
pub use crate::*;

// We need to include main.rs but avoid redefining main()
// Since lib crates don't need a main(), the fn main() in main.rs
// will only be compiled for the bin target.
#[path = "main.rs"]
mod __main_impl;
