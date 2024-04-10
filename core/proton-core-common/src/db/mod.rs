//! Core related database for user sessions and user info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.
use proton_sqlite3::{new_connection_wrapper, new_tracked_connection_wrapper, MigratorError};
use std::ops::Deref;

mod core;
mod migrations;
mod session;

pub use migrations::*;
pub use session::*;

pub use proton_sqlite3;

pub type DBResult<T> = proton_sqlite3::rusqlite::Result<T>;
pub type DBError = proton_sqlite3::rusqlite::Error;
pub type DBMigrationError = MigratorError;

new_tracked_connection_wrapper!(CoreSqliteConnection);
new_connection_wrapper!(SessionSqliteConnection);
