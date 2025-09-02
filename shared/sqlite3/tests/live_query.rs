#![allow(clippy::print_stdout)]
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::Stash;
use tokio::spawn as spawn_async;

#[derive(Clone, Debug, Deserialize, Model, PartialEq, Serialize)]
#[TableName("foo")]
pub struct Foo {
    #[IdField]
    pub id: u64,
    #[DbField]
    pub bar: u64,
}

struct FooWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for FooWatcher {
    fn tables(&self) -> Vec<String> {
        vec![Foo::table_name().to_string()]
    }
    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender.send(()).expect("failed to send");
    }
}

#[tokio::test]
async fn test_tracker() {
    let dir = tempdir::TempDir::new("sqlite3_test").expect("failed to create temp dir");
    let db_path = dir.path().join("sqlite.db");
    let stash = Stash::new(Some(&db_path)).expect("Failed to create Stash");
    let conn = stash.connection().await.unwrap();

    conn.execute(
        "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, bar INTEGER)",
        vec![],
    )
    .await
    .expect("failed to create table");

    let receiver = stash
        .subscribe_to(|sender| Box::new(FooWatcher { sender }))
        .unwrap()
        .receiver;

    let mut join_handles = Vec::new();
    for _ in 0..3 {
        let stash_clone = stash.clone();
        let h = spawn_async(async move {
            let mut conn = stash_clone.connection().await.unwrap();
            conn.tx(async |tx| {
                tx.execute("INSERT INTO foo VALUES (null, 10)", vec![])
                    .await
            })
            .await
            .expect("failed commit");
        });

        join_handles.push(h);
    }

    spawn_async(async move {
        for (i, h) in join_handles.into_iter().enumerate() {
            h.await
                .unwrap_or_else(|_| panic!("failed to join thread {i}"));
        }
    });

    let mut count = 0;
    while receiver.recv_async().await.is_ok() {
        count += 1;
        if count >= 3 {
            break;
        }
    }
}
