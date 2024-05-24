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

pub mod orm;
pub mod stash;
