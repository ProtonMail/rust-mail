//! Sql trait implementations for `sqlx`.
//!
//! Requires the `sqlx` feature to be enabled.

use crate::connection::SqlExecutorAsync;
use sqlx::{Row, SqliteConnection};

impl SqlExecutorAsync for SqliteConnection {
    type Error = sqlx::Error;

    async fn sql_query_values(&mut self, query: &str) -> Result<Vec<u32>, Self::Error> {
        let rows = sqlx::query(query).fetch_all(self).await?;
        Ok(rows.into_iter().map(|r| r.get::<u32, _>(0)).collect())
    }

    async fn sql_execute(&mut self, query: &str) -> Result<(), Self::Error> {
        sqlx::query(query).execute(self).await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::ConnectionAsync as WatchedConnection;
    use crate::connection::test::TestObserver;
    use crate::watcher::Watcher;
    use sqlx::{Connection, Sqlite, Transaction};
    use std::collections::BTreeSet;
    use std::sync::Arc;

    #[tokio::test]
    async fn transaction_tracking() {
        let orig = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            orig(panic_info);
            std::process::exit(-1);
        }));

        let mut connection = SqliteConnection::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)")
            .execute(&mut connection)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE bar (v INTEGER UNIQUE)")
            .execute(&mut connection)
            .await
            .unwrap();

        let watcher = Watcher::new().unwrap();
        let mut connection = WatchedConnection::new(connection, Arc::clone(&watcher))
            .await
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

        do_tx(&mut connection, async |tx| {
            sqlx::query("INSERT INTO foo VALUES( null,10)")
                .execute(&mut **tx)
                .await
                .unwrap();
        })
        .await;
        receiver.recv().unwrap();
        do_tx(&mut connection, async |tx| {
            sqlx::query("INSERT OR REPLACE INTO bar VALUES(10)")
                .execute(&mut **tx)
                .await
                .unwrap();
        })
        .await;
        receiver.recv().unwrap();
        do_tx(&mut connection, async |tx| {
            sqlx::query("INSERT OR REPLACE INTO bar VALUES(10)")
                .execute(&mut **tx)
                .await
                .unwrap();
        })
        .await;
        receiver.recv().unwrap();
        do_tx(&mut connection, async |tx| {
            sqlx::query("DELETE FROM foo WHERE v=10")
                .execute(&mut **tx)
                .await
                .unwrap();
            sqlx::query("DELETE FROM bar WHERE v=10")
                .execute(&mut **tx)
                .await
                .unwrap();
        })
        .await;
        receiver.recv().unwrap();

        connection.stop_tracking().await.unwrap();
    }

    async fn do_tx(
        connection: &mut WatchedConnection<SqliteConnection>,
        f: impl AsyncFnOnce(&mut Transaction<Sqlite>) -> (),
    ) {
        connection.sync_watcher_tables().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        {
            f(&mut tx).await;
        }
        tx.commit().await.unwrap();
        connection.publish_watcher_changes().await.unwrap();
    }
}
