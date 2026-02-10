//! Sql trait implementations for `rusqlite`.
//!
//! Requires the `rusqlite` feature to be enabled.
use crate::connection::SqlExecutor;
use rusqlite::Connection;

impl SqlExecutor for Connection {
    type Error = rusqlite::Error;
    fn sql_query_values(&self, query: &str) -> Result<Vec<u32>, Self::Error> {
        let mut stmt = self.prepare(query)?;
        let rows = stmt.query_map((), |r| r.get(0))?;
        let mut table_ids = Vec::new();
        for row in rows {
            table_ids.push(row?);
        }
        Ok(table_ids)
    }

    fn sql_execute(&self, query: &str) -> Result<(), Self::Error> {
        Connection::execute(self, query, ())?;
        Ok(())
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::test::TestObserver;
    use crate::connection::{Connection as WatchedConnection, State};
    use crate::statement::Statement;
    use crate::watcher::Watcher;
    use rusqlite::{Transaction, TransactionBehavior};
    use std::collections::BTreeSet;
    use std::sync::Arc;

    #[test]
    fn transaction_tracking() {
        let orig = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            orig(panic_info);
            std::process::exit(-1);
        }));

        let connection = Connection::open_in_memory().unwrap();

        let watcher = Watcher::new().unwrap();
        let mut connection = WatchedConnection::new(connection, Arc::clone(&watcher)).unwrap();
        connection
            .execute(
                "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)",
                (),
            )
            .unwrap();
        connection
            .execute("CREATE TABLE bar (v INTEGER UNIQUE)", ())
            .unwrap();

        let foo_table_set = BTreeSet::from_iter(["foo".to_string()]);
        let bar_table_set = BTreeSet::from_iter(["bar".to_string()]);
        let foo_bar_table_set = BTreeSet::from_iter(["foo".to_string(), "bar".to_string()]);

        // Synchronization to avoid merging of changes;
        let (observer, receiver) = TestObserver::new(
            foo_bar_table_set.clone().into_iter().collect(),
            [
                foo_table_set,
                bar_table_set.clone(),
                bar_table_set,
                foo_bar_table_set,
            ],
        );

        let _ = watcher.add_observer(Box::new(observer));

        do_tx(&mut connection, |tx| {
            tx.execute("INSERT INTO foo VALUES( null,10)", ()).unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            tx.execute("INSERT OR REPLACE INTO bar VALUES(10)", ())
                .unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            tx.execute("INSERT OR REPLACE INTO bar VALUES(10)", ())
                .unwrap();
        });
        receiver.recv().unwrap();
        do_tx(&mut connection, |tx| {
            tx.execute("DELETE FROM foo WHERE v=10", ()).unwrap();
            tx.execute("DELETE FROM bar WHERE v=10", ()).unwrap();
        });
        receiver.recv().unwrap();

        connection.stop_tracking().unwrap();
    }

    #[test]
    fn execute_temp_table_changes_when_transaction_is_open() {
        // create 2 connections, one holds a transaction open, the other
        // sync changes from the watcher. This should succeed without blocking the
        // other connection.
        let tmp_dir = tempdir::TempDir::new("sqlite").unwrap();
        let db_path = tmp_dir.path().join("test.db");
        let watcher = Watcher::new().unwrap();

        let connection1 = Connection::open(&db_path).unwrap();
        State::set_pragmas().execute(&connection1).unwrap();
        connection1
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection1
            .pragma_update(None, "busy_timeout", "50")
            .unwrap();
        let mut state = State::new();

        connection1
            .execute(
                "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)",
                (),
            )
            .unwrap();

        let (observer, _) = TestObserver::new(
            vec!["foo".to_string()],
            [BTreeSet::from_iter(["foo".to_string()])],
        );

        let _ = watcher.add_observer(Box::new(observer));

        // Connection 2 creates a immediate transaction which engages the writer lock.
        let mut connection2 = Connection::open(&db_path).unwrap();
        let _tx2 = connection2
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();

        // These 2 methods only manipulate temporary tables and triggers, they will execute
        // without triggering the sqlite busy errors.
        State::start_tracking().execute(&connection1).unwrap();
        state.sync_tables(&watcher).execute(&connection1).unwrap();
    }

    fn do_tx(connection: &mut WatchedConnection<Connection>, f: impl FnOnce(&mut Transaction)) {
        connection.sync_watcher_tables().unwrap();
        let mut tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        f(&mut tx);
        tx.commit().unwrap();
        connection.publish_watcher_changes().unwrap();
    }
}
