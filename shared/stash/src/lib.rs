#![allow(clippy::doc_markdown)]

//! Database-handling functionality.
//!
//! This crate provides a set of traits and structs for working with persistent
//! data stored in a SQLite database. It presents a simple, easy-to-use interface
//! for working with database records, in two layers:
//!
//!   - The database-handling layer, which provides a low-level interface for
//!     interacting with the database.
//!   - The record-handling layer, which provides a more convenient ORM-based
//!     interface for working with types that are saved to the database.
//!
//! Either of these layers can be used as appropriate, with the ORM layer being
//! suitable for simple record management tasks, and the database-handling layer
//! being available for more complex database operations.
//!

// Standard modules
pub(crate) mod connection_manager;
pub mod marker;
pub mod orm;
pub mod stash;
pub mod utils;
pub mod watcher;

#[allow(deprecated)]
pub use marker::{AccountDb, DefaultDb, UserDb};

/// Re-exported proc macros.
///
/// This module re-exports the proc macros defined in the `stash-macros` crate.
/// It is here for convenience, so that users of the macros do not need to
/// import them from the `stash-macros` crate directly.
///
pub mod macros {
    pub use stash_macros::DbRecord;
    pub use stash_macros::Model;
}

/// Re-exported external types.
///
/// This module re-exports types from external crates that are used in the
/// `stash` crate. This is done to make it easier for users of the `stash`
/// crate to access these types without needing to import them from the
/// external crates directly.
///
/// At present, the only types re-exported here are from the [`rusqlite`](https://crates.io/crates/rusqlite)
/// crate.
///
pub mod exports {
    pub use rusqlite::hooks::Action;
    pub use rusqlite::types::{
        FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef,
    };
    pub use rusqlite::{Connection, Error as SqliteError, Row, Transaction};
}
pub use rusqlite;

/// Use of crates that are used in integration tests, to prevent lint warnings.
#[cfg(test)]
mod integration_test_package_usage {
    use futures as _;
    use tempfile as _;
}
