use stash::params;
use stash::stash::{Bond, Stash, Tether};
use std::thread::spawn;
use std::time::Duration;
use tokio::spawn as async_spawn;
use tokio::time::sleep;

async fn create_table(tether: &Tether) {
    tether
        .execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();
}

async fn insert(tether: &Tether, value: &str) {
    tether
        .execute(
            r#"INSERT INTO test_kv (value) VALUES (?)"#,
            params![value.to_owned()],
        )
        .await
        .unwrap();
}

async fn query(tether: &Tether, value: &str) -> Vec<String> {
    tether
        .query_values::<_, String>(
            r#"SELECT value FROM test_kv WHERE value = ?"#,
            params![value.to_owned()],
        )
        .await
        .unwrap()
}

async fn create_table_tx(tx: &Bond<'_>) {
    tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();
}

async fn insert_tx(tx: &Bond<'_>, value: &str) {
    tx.execute(
        r#"INSERT INTO test_kv (value) VALUES (?)"#,
        params![value.to_owned()],
    )
    .await
    .unwrap();
}

async fn query_tx(tx: &Bond<'_>, value: &str) -> Vec<String> {
    tx.query_values::<_, String>(
        r#"SELECT value FROM test_kv WHERE value = ?"#,
        params![value.to_owned()],
    )
    .await
    .unwrap()
}

#[cfg(test)]
mod concurrency_basic_sync {
    use stash::stash::Stash;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let conn = stash.connection().await.unwrap();

        // Create a table
        conn.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
            .await
            .unwrap();

        // Insert some data
        conn.execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
            .await
            .unwrap();

        // Query the data
        let result = conn
            .query_values::<_, String>(r#"SELECT value FROM test_kv WHERE value = "test""#, vec![])
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut conn = stash.connection().await.unwrap();
        let result = conn
            .tx(async |tx| {
                // Create a table
                tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
                    .await
                    .unwrap();

                // Insert some data
                tx.execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
                    .await
                    .unwrap();

                // Query the data
                tx.query_values::<_, String>(
                    r#"SELECT value FROM test_kv WHERE value = "test""#,
                    vec![],
                )
                .await
            })
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn basic_query_with_transaction_closure_spawnd() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let result = tokio::spawn(async move {
            let mut conn = stash.connection().await.unwrap();
            conn.tx(async |tx| {
                // Create a table
                tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
                    .await
                    .unwrap();

                // Insert some data
                tx.execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
                    .await
                    .unwrap();

                // Query the data
                tx.query_values::<_, String>(
                    r#"SELECT value FROM test_kv WHERE value = "test""#,
                    vec![],
                )
                .await
            })
            .await
            .unwrap()
        })
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }
}

#[cfg(test)]
mod concurrency_async_functions {
    use super::*;
    use stash::stash::StashError;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let conn = stash.connection().await.unwrap();

        create_table(&conn).await;
        insert(&conn, "test").await;
        let result = query(&conn, "test").await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut conn = stash.connection().await.unwrap();

        let result = conn
            .tx::<_, _, StashError>(async |tx| {
                create_table_tx(tx).await;
                insert_tx(tx, "test").await;
                Ok(query_tx(tx, "test").await)
            })
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }
}

#[cfg(test)]
mod concurrency_async_threads {
    use super::*;
    use stash::stash::StashError;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let conn = stash.connection().await.unwrap();

        let result = async_spawn(async move {
            create_table(&conn).await;
            insert(&conn, "test").await;
            query(&conn, "test").await
        })
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let result = async_spawn(async move {
            let mut conn = stash.connection().await.unwrap();
            conn.tx::<_, _, StashError>(async |tx| {
                create_table_tx(tx).await;
                insert_tx(tx, "test").await;
                Ok(query_tx(tx, "test").await)
            })
            .await
            .unwrap()
        })
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let stash1 = stash.clone();
        let stash2 = stash.clone();
        let conn = stash.connection().await.unwrap();

        create_table(&conn).await;

        // First thread, with first transaction
        let handle1 = async_spawn(async move {
            let mut conn1 = stash1.connection().await.unwrap();
            conn1
                .tx::<_, _, StashError>(async |tx| {
                    insert_tx(tx, "test1").await;
                    Ok(query_tx(tx, "test1").await)
                })
                .await
                .unwrap()
        });

        // Second thread, with second transaction
        let handle2 = async_spawn(async move {
            let mut conn2 = stash2.connection().await.unwrap();
            conn2
                .tx::<_, _, StashError>(async |tx| {
                    insert_tx(tx, "test2").await;
                    Ok(query_tx(tx, "test2").await)
                })
                .await
                .unwrap()
        });

        // Wait for the threads to complete
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = conn
            .query_values::<_, String>(r#"SELECT value FROM test_kv ORDER BY value"#, vec![])
            .await
            .unwrap();

        assert_eq!(result1.len(), 1);
        assert_eq!(result1[0], "test1".to_owned());
        assert_eq!(result2.len(), 1);
        assert_eq!(result2[0], "test2".to_owned());
        assert_eq!(result3.len(), 2);
        assert_eq!(result3[0], "test1".to_owned());
        assert_eq!(result3[1], "test2".to_owned());
    }
}

#[cfg(test)]
mod concurrency_std_threads {
    use super::*;
    use stash::stash::StashError;
    use tokio::runtime::Runtime;

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let stash1 = stash.clone();
        let stash2 = stash.clone();
        let conn = stash.connection().await.unwrap();

        create_table(&conn).await;

        // First thread, with first transaction
        let handle1 = spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut conn1 = stash1.connection().await.unwrap();
                conn1
                    .tx::<_, _, StashError>(async |tx| {
                        insert_tx(tx, "test1").await;
                        Ok(query_tx(tx, "test1").await)
                    })
                    .await
                    .unwrap()
            })
        });

        // Second thread, with second transaction
        let handle2 = spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut conn2 = stash2.connection().await.unwrap();
                conn2
                    .tx::<_, _, StashError>(async |tx| {
                        insert_tx(tx, "test2").await;
                        Ok(query_tx(tx, "test2").await)
                    })
                    .await
                    .unwrap()
            })
        });

        // Wait for the threads to complete
        let result1 = handle1.join().unwrap();
        let result2 = handle2.join().unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = conn
            .query_values::<_, String>(r#"SELECT value FROM test_kv ORDER BY value"#, vec![])
            .await
            .unwrap();

        assert_eq!(result1.len(), 1);
        assert_eq!(result1[0], "test1".to_owned());
        assert_eq!(result2.len(), 1);
        assert_eq!(result2[0], "test2".to_owned());
        assert_eq!(result3.len(), 2);
        assert_eq!(result3[0], "test1".to_owned());
        assert_eq!(result3[1], "test2".to_owned());
    }
}

#[cfg(test)]
mod concurrency_mixed {
    use super::*;
    use stash::stash::StashError;
    use tokio::runtime::Runtime;

    #[tokio::test]
    async fn basic_query_with_multiple_mixed_simultaneous_approaches() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let conn = stash.connection().await.unwrap();
        let stash1 = stash.clone();
        let stash2 = stash.clone();
        let stash3 = stash.clone();
        let stash4 = stash.clone();
        let stash5 = stash.clone();
        let stash6 = stash.clone();
        let stash7 = stash.clone();
        let stash8 = stash.clone();
        let stash9 = stash.clone();

        create_table(&conn).await;

        // First thread (std), with first transaction
        let handle1 = spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut conn1 = stash1.connection().await.unwrap();
                conn1
                    .tx::<_, _, StashError>(async |tx| {
                        insert_tx(tx, "test1").await;
                        let result = query_tx(tx, "test1").await;
                        sleep(Duration::from_millis(100)).await;
                        Ok(result)
                    })
                    .await
                    .unwrap()
            })
        });

        // Second thread (std), with second transaction
        let handle2 = spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let mut conn2 = stash2.connection().await.unwrap();
                conn2
                    .tx::<_, _, StashError>(async |tx| {
                        insert_tx(tx, "test2").await;
                        Ok(query(tx, "test2").await)
                    })
                    .await
                    .unwrap()
            })
        });

        // Third thread (std), with no transaction
        let handle3 = spawn(move || {
            Runtime::new().unwrap().block_on(async {
                let conn3 = stash3.connection().await.unwrap();
                insert(&conn3, "test3").await;
                sleep(Duration::from_millis(100)).await;
                query(&conn3, "test3").await
            })
        });

        // Fourth thread (async), with third transaction
        let handle4 = async_spawn(async move {
            let mut conn4 = stash4.connection().await.unwrap();
            conn4
                .tx::<_, _, StashError>(async |tx| {
                    insert_tx(tx, "test4").await;
                    let result = query_tx(tx, "test4").await;
                    sleep(Duration::from_millis(100)).await;
                    Ok(result)
                })
                .await
                .unwrap()
        });

        // Fifth thread (async), with fourth transaction
        let handle5 = async_spawn(async move {
            let mut conn5 = stash5.connection().await.unwrap();
            conn5
                .tx::<_, _, StashError>(async |tx| {
                    insert_tx(tx, "test5").await;
                    sleep(Duration::from_millis(100)).await;
                    Ok(query(tx, "test5").await)
                })
                .await
                .unwrap()
        });

        // Sixth thread (async), with no transaction
        let handle6 = async_spawn(async move {
            let conn6 = stash6.connection().await.unwrap();
            insert(&conn6, "test6").await;
            sleep(Duration::from_millis(100)).await;
            query(&conn6, "test6").await
        });

        // Wait for the threads to complete
        let result1 = handle1.join().unwrap();
        let result2 = handle2.join().unwrap();
        let result3 = handle3.join().unwrap();
        let result4 = handle4.await.unwrap();
        let result5 = handle5.await.unwrap();
        let result6 = handle6.await.unwrap();

        let conn7 = stash7.connection().await.unwrap();
        // Additional write queries
        conn7
            .execute(r#"INSERT INTO test_kv (value) VALUES ("test7")"#, vec![])
            .await
            .unwrap();
        let conn8 = stash8.connection().await.unwrap();
        insert(&conn8, "test8").await;
        let mut conn9 = stash9.connection().await.unwrap();
        let result9 = conn9
            .tx::<_, _, StashError>(async |tx| {
                insert_tx(tx, "test9").await;
                Ok(query_tx(tx, "test9").await)
            })
            .await
            .unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result7 = query(&conn, "test7").await;
        let result8 = conn
            .query_values::<_, String>(r#"SELECT value FROM test_kv ORDER BY value"#, vec![])
            .await
            .unwrap();

        assert_eq!(result1.len(), 1);
        assert_eq!(result1[0], "test1".to_owned());
        assert_eq!(result2.len(), 1);
        assert_eq!(result2[0], "test2".to_owned());
        assert_eq!(result3.len(), 1);
        assert_eq!(result3[0], "test3".to_owned());
        assert_eq!(result4.len(), 1);
        assert_eq!(result4[0], "test4".to_owned());
        assert_eq!(result5.len(), 1);
        assert_eq!(result5[0], "test5".to_owned());
        assert_eq!(result6.len(), 1);
        assert_eq!(result6[0], "test6".to_owned());
        assert_eq!(result7.len(), 1);
        assert_eq!(result7[0], "test7".to_owned());
        assert_eq!(result9.len(), 1);
        assert_eq!(result9[0], "test9".to_owned());
        assert_eq!(result8.len(), 9);
        assert_eq!(result8[0], "test1".to_owned());
        assert_eq!(result8[7], "test8".to_owned());
    }
}

mod orm_tests {
    use crate::params;
    use rusqlite::{Connection, Transaction};
    use stash::{
        orm::{Model, ModelHooks},
        stash::{Stash, StashError},
        utils::ConnectionExt,
    };
    use stash_macros::Model;

    #[derive(Clone, Debug, Eq, Model, PartialEq)]
    #[TableName("my_model")]
    #[ModelHooks]
    struct MyModel {
        #[IdField(autoincrement)]
        id: Option<u64>,

        /// Keeps track of all queries. should be equal among all records.
        #[DbField]
        all_rustaceans: u64,

        /// Mascot. One is ferris and the other one should be corro
        #[DbField]
        mascot: String,
        other_mascot: String,

        #[DbField]
        rustacean: String,
    }

    impl ModelHooks for MyModel {
        fn before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
            if self.mascot == "ferris" {
                assert_eq!(self.other_mascot, "corro")
            } else if self.mascot == "corro" {
                assert_eq!(self.other_mascot, "ferris")
            } else {
                panic!("unknown mascot {}", self.mascot);
            }
            self.all_rustaceans = tx.query_row_col("SELECT COUNT(*) from my_model", ())?;
            Ok(())
        }

        fn after_save(&mut self, bond: &Transaction<'_>) -> Result<(), StashError> {
            bond.execute(
                "UPDATE my_model SET all_rustaceans = all_rustaceans + 1",
                (),
            )?;
            self.all_rustaceans += 1;
            Ok(())
        }

        fn after_load(&mut self, _: &Connection) -> Result<(), StashError> {
            if self.mascot == "ferris" {
                self.other_mascot = "corro".to_string();
            } else if self.mascot == "corro" {
                self.other_mascot = "ferris".to_owned();
            } else {
                panic!("unknown mascot {}", self.mascot);
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_orm() -> anyhow::Result<()> {
        let stash = Stash::new(None)?;
        let mut tether = stash.connection().await?;

        tether
            .tx::<_, _, StashError>(async |tx| {
                tx.execute(
                    r#"CREATE TABLE my_model
            (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                all_rustaceans INTEGER NOT NULL,
                mascot TEXT NOT NULL,
                rustacean TEXT NOT NULL
            )"#,
                    vec![],
                )
                .await
            })
            .await
            .unwrap();

        let mut boats = MyModel {
            id: None,
            all_rustaceans: 0,
            mascot: "ferris".to_owned(),
            other_mascot: "corro".to_owned(),
            rustacean: "without boats".to_string(),
        };
        let mut niko = MyModel {
            id: None,
            all_rustaceans: 0,
            mascot: "corro".to_owned(),
            other_mascot: "ferris".to_owned(),
            rustacean: "niko matsakis".to_string(),
        };

        tether
            .tx::<_, _, StashError>(async |tx| {
                boats.save(tx).await?;
                niko.save(tx).await?;

                // Expected it to be broken
                assert_eq!(boats.all_rustaceans, 1);
                assert_eq!(niko.all_rustaceans, 2);

                let boats2 = MyModel::find_first("WHERE id = ?", params![boats.id], tx)
                    .await?
                    .unwrap();
                let niko2 = MyModel::find_first("WHERE id = ?", params![niko.id], tx)
                    .await?
                    .unwrap();

                // Manual update
                boats.all_rustaceans = 2;

                assert_eq!(boats, boats2);
                assert_eq!(niko, niko2);
                Ok(())
            })
            .await
            .unwrap();
        Ok(())
    }
}

mod interrupt {
    use super::*;

    #[tokio::test]
    async fn transactions_are_interrupted() -> anyhow::Result<()> {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let mut conn = stash.connection().await?;
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        let (sender_interrupt, receiver_interrupt) = tokio::sync::oneshot::channel::<()>();
        let join_handle = tokio::spawn(async move {
            conn.tx(async move |tx| {
                sender.send(()).unwrap();
                receiver_interrupt.await.unwrap();
                tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
                    .await
            })
            .await
        });

        receiver.await?;
        stash.interrupt();
        sender_interrupt.send(()).unwrap();

        let err = join_handle.await?.unwrap_err();
        assert!(err.was_interrupt());

        Ok(())
    }

    #[tokio::test]
    async fn new_transactions_wait_until_resume() -> anyhow::Result<()> {
        let db_dir = tempfile::tempdir().unwrap();
        let stash: Stash =
            Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        stash.interrupt();
        let mut conn = stash.connection().await?;
        let stash_cloned = stash.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            sender.send(()).unwrap();
            sleep(Duration::from_millis(400)).await;
            stash_cloned.resume();
        });

        receiver.await?;

        tokio::time::timeout(
            Duration::from_secs(1),
            conn.tx(async |tx| {
                tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
                    .await
            }),
        )
        .await
        .unwrap()?;

        Ok(())
    }
}
