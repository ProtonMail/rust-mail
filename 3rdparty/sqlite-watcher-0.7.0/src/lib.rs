//! This crate provides the basic building blocks to observe changes in a sqlite database
//! similar to Room (Android) and Core Data (iOS). Additional features such as observable
//! queries are not included, but can potentially be built using the provided types.
//!
//! The crate is agnostic over the implementation of the sqlite connection. Example
//! implementations are provided for [`rusqlite`](https://crates.io/crates/rusqlite) and
//! [`sqlx`](https://crates.io/crates/sqlx) which need to be enabled by the respectively named
//! features.
//!
//! # Basic example
//!
//! ```rust
//! use std::collections::BTreeSet;
//! use std::sync::Arc;
//! use sqlite_watcher::connection::Connection;
//! use sqlite_watcher::watcher::{TableObserver, Watcher};
//!
//! struct MyObserver{}
//!
//! impl TableObserver for MyObserver {
//!     fn tables(&self) -> Vec<String> {
//!         vec!["foo".to_owned()]
//!     }
//!
//!     fn on_tables_changed(&self, tables: &BTreeSet<String>) {
//!         println!("Tables updated: {tables:?}")
//!     }
//! }
//!
//! // create a watcher
//! let watcher = Watcher::new().unwrap();
//! let mut sql_connection = rusqlite::Connection::open_in_memory().unwrap();
//! let mut connection = Connection::new(sql_connection, Arc::clone(&watcher)).unwrap();
//! // Create table
//! connection.execute("CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, value INTEGER)", ()) .unwrap();
//! // Register observer
//! let handle = watcher.add_observer(Box::new(MyObserver{}));
//! // Sync changes from watcher so we start watching 'foo'
//! connection.sync_watcher_tables().unwrap();
//! // Modify table
//! connection.execute("INSERT INTO foo (value) VALUES (10)", ()) .unwrap();
//! // Check and publish changes.
//! connection.publish_watcher_changes().unwrap();
//! // MyObserver::on_tables_changed should be called at some point.
//! // Sync changes from watcher so we are up to date
//! connection.sync_watcher_tables().unwrap();
//! // Modify table
//! connection.execute("INSERT INTO foo (value) VALUES (20)", ()) .unwrap();
//! // Check and publish changes.
//! connection.publish_watcher_changes().unwrap();
//! // MyObserver::on_tables_changed should be called at some point.
//!
//! ```
//!
//! # How it works
//!
//! The crate creates a temporary table in each database connection when changes are recorded. For
//! every table that we observe via a [`TableObserver`], we create temporary triggers for
//! INSERT, UPDATE and DELETE queries. These queries are create and removed when calling
//! [`connection::Connection::sync_watcher_tables`] or [`connection::State::sync_tables`].
//!
//! When such as query runs, for a given table, we mark it as updated.
//! [`connection::Connection::publish_watcher_changes()`] or [`connection::State::publish_changes`]
//! check the tracking table and notifies the [Watcher] of all the tables that have been modified.
//!
//! Finally in a background thread, the [Watcher] invokes
//! [`watcher::TableObserver::on_tables_changed()`]
//! for each observer which is watching the modified table.
//!
//! # Change Granularity
//!
//! The [`TableObserver`] is only notified that the table it wants to observe was modified.
//! It does not include information pertaining to which row was affected or which type of operation
//! triggered the change (INSERT/UPDATE/DELETE).
//!
//! # Multiple Connections
//!
//! The [Watcher] can be used with one or multiple connections. Each connection will publish
//! their changes to the [Watcher] assigned to it.
//!
//! # Singe Process
//!
//! The only limitation of this model is that it only works for connections that inhabit the same
//! process space. While sqlite supports being modified by multiple processes, the current observation
//! does not support this use case.
//!
//! # Custom Integrations
//!
//! This crate was designed so that it can easily be integrated in existing projects and or
//! connection libraries. [State] contains everything that is required to patch
//! an existing connection.
//!
//! [Connection] is an example implementation that ties everything together.
//!
//! # Async
//!
//! While the [Watcher] is a mostly a sync implementation, you can still use the tracking
//! machinery with async connections. For every method in [State] there is an async counterpart which
//! ends with the `_async` suffix. There is also an asynchronous connection implementation
//! ([`ConnectionAsync`]).
//!
//! If you need to run async code as part of the [`TableObserver`], it's recommended to either
//! spawn an async task on the runtime of your choice or channel it to a dedicated async context
//! via a channel such as [`flume`](https://crates.io/crates/flume).
//!
//! [Watcher]: `watcher::Watcher`
//! [State]: `connection::State`
//! [`ConnectionAsync`]: `connection::ConnectionAsync`
//! [Connection]: `connection::Connection`
//! [`TableObserver`]: `watcher::TableObserver`
//!

pub mod connection;
pub mod statement;
pub mod watcher;
