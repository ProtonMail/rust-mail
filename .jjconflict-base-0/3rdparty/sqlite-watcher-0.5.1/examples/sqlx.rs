use sqlite_watcher::connection::ConnectionAsync;
use sqlite_watcher::watcher::{TableObserver, Watcher};
use sqlx::Connection;
use std::collections::BTreeSet;
use std::sync::Arc;
use tempdir::TempDir;
use tokio::sync::mpsc::Sender;

// Simple example which starts 2 connections on the same database with 2 observers.
// It should print at least one entry for each observer:
// ```
// Updated tables: [observer-1] {"foo"}
// Updated tables: [observer-2] {"foo"}
// ```
#[tokio::main]
async fn main() {
    let tmp_dir = TempDir::new("sqlite-watcher-rusqlite").unwrap();
    let db_file = tmp_dir.path().join("db.sqlite3");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_file)
        .create_if_missing(true);
    let connection1 = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let connection2 = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let watcher = Watcher::new().unwrap();

    let mut connection1 = ConnectionAsync::new(connection1, Arc::clone(&watcher))
        .await
        .unwrap();
    let connection2 = ConnectionAsync::new(connection2, Arc::clone(&watcher))
        .await
        .unwrap();

    let (sender, mut receiver) = tokio::sync::mpsc::channel(5);

    let observer1 = Observer::new("observer-1", sender.clone());
    let observer2 = Observer::new("observer-2", sender);

    let observer_handle_1 = watcher.add_observer(Box::new(observer1)).unwrap();
    let observer_handle_2 = watcher.add_observer(Box::new(observer2)).unwrap();

    sqlx::query("CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, value INTEGER)")
        .execute(&mut *connection1)
        .await
        .unwrap();

    let thread_handles = [
        (connection1, observer_handle_1),
        (connection2, observer_handle_2),
    ]
    .into_iter()
    .map(|(mut connection, observer_handle)| {
        let watcher_cloned = Arc::clone(&watcher);
        tokio::spawn(async move {
            connection.sync_watcher_tables().await.unwrap();
            let mut tx = connection.begin().await.unwrap();
            sqlx::query("INSERT INTO foo (value) VALUES (?)")
                .bind(400)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            connection.publish_watcher_changes().await.unwrap();
            watcher_cloned.remove_observer(observer_handle).unwrap();
        })
    })
    .collect::<Vec<_>>();

    while let Some((observer_name, updated_tables)) = receiver.recv().await {
        println!("Updated tables: [{observer_name}] {updated_tables:?}")
    }

    for thread_handle in thread_handles {
        thread_handle.await.unwrap();
    }
}

struct Observer {
    name: String,
    sender: Sender<(String, BTreeSet<String>)>,
}

impl Observer {
    pub fn new(name: impl Into<String>, sender: Sender<(String, BTreeSet<String>)>) -> Observer {
        Self {
            name: name.into(),
            sender,
        }
    }
}

impl TableObserver for Observer {
    fn tables(&self) -> Vec<String> {
        vec!["foo".to_owned()]
    }

    fn on_tables_changed(&self, tables: &BTreeSet<String>) {
        self.sender
            .blocking_send((self.name.clone(), tables.clone()))
            .unwrap()
    }
}
