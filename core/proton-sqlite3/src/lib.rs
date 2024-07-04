//! DEPRECATED
//!

mod migration;
mod tracker;

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;

pub use migration::*;

// re-export;
pub use rusqlite;
