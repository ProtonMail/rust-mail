# sqlite-watcher


This crate provides the basic building blocks to observe changes in a sqlite database
similar to Room (Android) and Core Data (iOS). Additional features such as observable
queries are not included, but can potentially be built using the provided types.

The crate is agnostic over the implementation of the sqlite connection. Example
implementations are provided for [`rusqlite`](https://crates.io/crates/rusqlite) and
[`sqlx`](https://crates.io/crates/sqlx) which need to be enabled by the respectively named
features.

 ## Basic example

 ```rust
 use std::collections::BTreeSet;
 use std::sync::Arc;
 use sqlite_watcher::connection::Connection;
 use sqlite_watcher::watcher::{TableObserver, Watcher};

 struct MyObserver{}

 impl TableObserver for MyObserver {
     fn tables(&self) -> Vec<String> {
         vec!["foo".to_owned()]
     }

     fn on_tables_changed(&self, tables: &BTreeSet<String>) {
         println!("Tables updated: {tables:?}")
     }
 }

 // create a watcher
 let watcher = Watcher::new().unwrap();
 let mut sql_connection = rusqlite::Connection::open_in_memory().unwrap();
 let mut connection = Connection::new(sql_connection, Arc::clone(&watcher)).unwrap();
 // Create table
 connection.execute("CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, value INTEGER)", ()) .unwrap();
 // Register observer
 let handle = watcher.add_observer(Box::new(MyObserver{}));
 // Sync changes from watcher so we start watching 'foo'
 connection.sync_watcher_tables().unwrap();
 // Modify table
 connection.execute("INSERT INTO foo (value) VALUES (10)", ()) .unwrap();
 // Check and publish changes.
 connection.publish_watcher_changes().unwrap();
 // MyObserver::on_tables_changed should be called at some point.
 // Sync changes from watcher so we are up to date
 connection.sync_watcher_tables().unwrap();
 // Modify table
 connection.execute("INSERT INTO foo (value) VALUES (20)", ()) .unwrap();
 // Check and publish changes.
 connection.publish_watcher_changes().unwrap();
 // MyObserver::on_tables_changed should be called at some point.

 ```

 ## Usage

 This crate was designed so that it can easily be integrated in existing projects and or
 connection libraries. [`connection::State`] contains everything that is required to patch
 an existing connection.

 [`connection::Connection`] is an example implementation that ties everything together.

## More Details

See the documentation and the examples for more details.
