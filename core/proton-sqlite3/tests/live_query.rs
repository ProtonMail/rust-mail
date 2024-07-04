use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::stash::Stash;
use tokio::spawn as spawn_async;

#[derive(Clone, Debug, Deserialize, Model, PartialEq, Serialize)]
#[TableName("foo")]
pub struct Foo {
    #[IdField]
    pub id: u64,
    #[DbField]
    pub bar: u64,
    #[RowIdField]
    #[serde(skip)]
    row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    stash: Option<Stash>,
}

#[tokio::test]
async fn test_tracker() {
    let dir = tempdir::TempDir::new("sqlite3_test").expect("failed to create temp dir");
    let db_path = dir.path().join("sqlite.db");
    let stash = Stash::new(Some(&db_path)).expect("Failed to create Stash");

    stash
        .execute(
            "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, bar INTEGER)",
            vec![],
        )
        .await
        .expect("failed to create table");

    let (sender, receiver) = flume::unbounded::<ResultsetChange<Foo, u64>>();
    let results = Foo::find("".to_owned(), vec![], &stash, Some(sender))
        .await
        .expect("Failed to run query");
    println!(">> {:?}", results);

    let mut join_handles = Vec::new();
    for _ in 0..3 {
        let stash_clone = stash.clone();
        let h = spawn_async(async move {
            stash_clone
                .execute("INSERT INTO foo VALUES (null, 10)", vec![])
                .await
                .expect("failed tx");
        });

        join_handles.push(h);
    }

    spawn_async(async move {
        for (i, h) in join_handles.into_iter().enumerate() {
            h.await.expect(&format!("failed to join thread {i}"));
        }
    });

    let mut count = 0;
    loop {
        match receiver.recv_async().await {
            Ok(change) => {
                println!(">> {:?}", change);
                count = count + 1;
                if count >= 3 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
