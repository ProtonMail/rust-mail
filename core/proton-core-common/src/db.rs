//! Core related database for user sessions and user info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.

mod addresses;
mod contacts;
mod core;
mod migrations;
pub(crate) mod session;

#[cfg(test)]
#[path = "tests/db.rs"]
mod tests;

pub use migrations::*;
pub use session::*;

pub use proton_sqlite3;
