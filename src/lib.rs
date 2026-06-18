//! eWatts Protocol — Library crate
//! Re-exports all public items from main.rs so that external tools
//! (fuzz targets, external tests) can link against the crate.
//! The lib target is kept in sync with the bin target by including
//! main.rs as a child module. Since lib crates have no main(),
//! the fn main() is only compiled in bin builds.

#![allow(unused_imports, dead_code)]

#[path = "main.rs"]
mod __main_impl;

// Re-export everything from __main_impl so that `use crate::*` and
// `use crate::module_name` work identically in both bin and lib targets.
pub use __main_impl::*;
