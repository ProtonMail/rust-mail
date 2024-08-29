#![allow(non_snake_case)]

use futures::executor::block_on;
use rusqlite::hooks::Action;
use stash::params;
use stash::stash::{Interface, Stash, Tether};
use std::thread::spawn;
use std::time::Duration;
use tokio::spawn as async_spawn;
use tokio::time::sleep;

async fn create_table(stash: &Stash) {
    stash
        .execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();
}

async fn insert(stash: &Stash, value: &str) {
    stash
        .execute(
            r#"INSERT INTO test_kv (value) VALUES (?)"#,
            params![value.to_owned()],
        )
        .await
        .unwrap();
}

async fn query(stash: &Stash, value: &str) -> Vec<String> {
    stash
        .query_values::<_, String>(
            r#"SELECT value FROM test_kv WHERE value = ?"#,
            params![value.to_owned()],
        )
        .await
        .unwrap()
}

async fn create_table_tx(tx: &Tether) {
    tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();
}

async fn insert_tx(tx: &Tether, value: &str) {
    tx.execute(
        r#"INSERT INTO test_kv (value) VALUES (?)"#,
        params![value.to_owned()],
    )
    .await
    .unwrap();
}

async fn query_tx(tx: &Tether, value: &str) -> Vec<String> {
    tx.query_values::<_, String>(
        r#"SELECT value FROM test_kv WHERE value = ?"#,
        params![value.to_owned()],
    )
    .await
    .unwrap()
}

#[cfg(test)]
mod concurrency_basic_sync {
    use super::*;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        // Create a table
        stash
            .execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
            .await
            .unwrap();

        // Insert some data
        stash
            .execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
            .await
            .unwrap();

        // Query the data
        let result = stash
            .query_values::<_, String>(r#"SELECT value FROM test_kv WHERE value = "test""#, vec![])
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        // Start a transaction
        let tx = stash
            .transaction()
            .await
            .expect("Failed to start transaction");

        // Create a table
        tx.execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
            .await
            .unwrap();

        // Insert some data
        tx.execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
            .await
            .unwrap();

        // Query the data
        let result = tx
            .query_values::<_, String>(r#"SELECT value FROM test_kv WHERE value = "test""#, vec![])
            .await
            .unwrap();

        // Commit the transaction
        tx.commit().await.expect("Failed to commit transaction");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        // Start two transactions
        let tx1 = stash
            .transaction()
            .await
            .expect("Failed to start transaction");
        let tx2 = stash
            .transaction()
            .await
            .expect("Failed to start transaction");

        // Create a table (not using transactions)
        stash
            .execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
            .await
            .unwrap();

        // Insert some data with transaction 1
        tx1.execute(r#"INSERT INTO test_kv (value) VALUES ("test1")"#, vec![])
            .await
            .unwrap();

        // Query the data, from the uncommitted transaction
        let result1 = tx1
            .query_values::<_, String>(r#"SELECT value FROM test_kv WHERE value = "test1""#, vec![])
            .await
            .unwrap();

        // Commit the transaction
        tx1.commit().await.expect("Failed to commit transaction");

        // Insert some more data with transaction 2
        tx2.execute(r#"INSERT INTO test_kv (value) VALUES ("test2")"#, vec![])
            .await
            .unwrap();

        // Commit the transaction
        tx2.commit().await.expect("Failed to commit transaction");

        // Query the data, re-using the transaction connections
        let result2 = tx2
            .query_values::<_, String>(r#"SELECT value FROM test_kv WHERE value = "test2""#, vec![])
            .await
            .unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = stash
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
mod concurrency_async_functions {
    use super::*;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        create_table(&stash).await;
        insert(&stash, "test").await;
        let result = query(&stash, "test").await;

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let tx = stash
            .transaction()
            .await
            .expect("Failed to start transaction");
        create_table_tx(&tx).await;
        insert_tx(&tx, "test").await;
        let result = query_tx(&tx, "test").await;
        tx.commit().await.expect("Failed to commit transaction");

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        // Start two transactions
        let tx1 = stash
            .transaction()
            .await
            .expect("Failed to start transaction");
        let tx2 = stash
            .transaction()
            .await
            .expect("Failed to start transaction");

        // Create a table (not using transactions)
        create_table(&stash).await;

        // Insert some data with transaction 1
        insert_tx(&tx1, "test1").await;

        // Query the data, from the uncommitted transaction
        let result1 = query_tx(&tx1, "test1").await;

        // Commit the transaction
        tx1.commit().await.expect("Failed to commit transaction");

        // Insert some more data with transaction 2
        insert_tx(&tx2, "test2").await;

        // Commit the transaction
        tx2.commit().await.expect("Failed to commit transaction");

        // Query the data, re-using the transaction connections
        let result2 = query_tx(&tx2, "test2").await;

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = stash
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
mod concurrency_async_threads {
    use super::*;

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let result = async_spawn(async move {
            create_table(&stash).await;
            insert(&stash, "test").await;
            query(&stash, "test").await
        })
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let result = async_spawn(async move {
            let tx = stash
                .transaction()
                .await
                .expect("Failed to start transaction");
            create_table_tx(&tx).await;
            insert_tx(&tx, "test").await;
            let result = query_tx(&tx, "test").await;
            tx.commit().await.expect("Failed to commit transaction");
            result
        })
        .await
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let stash1 = stash.clone();
        let stash2 = stash.clone();

        create_table(&stash).await;

        // First thread, with first transaction
        let handle1 = async_spawn(async move {
            let tx1 = stash1
                .transaction()
                .await
                .expect("Failed to start transaction");
            insert_tx(&tx1, "test1").await;
            let result = query_tx(&tx1, "test1").await;
            tx1.commit().await.expect("Failed to commit transaction");
            result
        });

        // Second thread, with second transaction
        let handle2 = async_spawn(async move {
            let tx2 = stash2
                .transaction()
                .await
                .expect("Failed to start transaction");
            insert_tx(&tx2, "test2").await;
            tx2.commit().await.expect("Failed to commit transaction");
            query_tx(&tx2, "test2").await
        });

        // Wait for the threads to complete
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = stash
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

    #[tokio::test]
    async fn basic_query_without_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let result = spawn(move || {
            block_on(async {
                create_table(&stash).await;
                insert(&stash, "test").await;
                query(&stash, "test").await
            })
        })
        .join()
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

        let result = spawn(move || {
            block_on(async {
                let tx = stash
                    .transaction()
                    .await
                    .expect("Failed to start transaction");
                create_table_tx(&tx).await;
                insert_tx(&tx, "test").await;
                let result = query_tx(&tx, "test").await;
                tx.commit().await.expect("Failed to commit transaction");
                result
            })
        })
        .join()
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "test".to_owned());
    }

    #[tokio::test]
    async fn basic_query_with_two_simultaneous_transactions() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let stash1 = stash.clone();
        let stash2 = stash.clone();

        create_table(&stash).await;

        // First thread, with first transaction
        let handle1 = spawn(move || {
            block_on(async {
                let tx1 = stash1
                    .transaction()
                    .await
                    .expect("Failed to start transaction");
                insert_tx(&tx1, "test1").await;
                let result = query_tx(&tx1, "test1").await;
                tx1.commit().await.expect("Failed to commit transaction");
                result
            })
        });

        // Second thread, with second transaction
        let handle2 = spawn(move || {
            block_on(async {
                let tx2 = stash2
                    .transaction()
                    .await
                    .expect("Failed to start transaction");
                insert_tx(&tx2, "test2").await;
                tx2.commit().await.expect("Failed to commit transaction");
                query_tx(&tx2, "test2").await
            })
        });

        // Wait for the threads to complete
        let result1 = handle1.join().unwrap();
        let result2 = handle2.join().unwrap();

        // Query the data, using the main Stash (no specific connection or transaction)
        let result3 = stash
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
    use tokio::runtime::Runtime;

    #[tokio::test]
    async fn basic_query_with_multiple_mixed_simultaneous_approaches() {
        let db_dir = tempfile::tempdir().unwrap();
        let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");
        let stash1 = stash.clone();
        let stash2 = stash.clone();
        let stash3 = stash.clone();
        let stash4 = stash.clone();
        let stash5 = stash.clone();
        let stash6 = stash.clone();
        let stash7 = stash.clone();
        let stash8 = stash.clone();
        let stash9 = stash.clone();

        create_table(&stash).await;

        // First thread (std), with first transaction
        let handle1 = spawn(move || {
            Runtime::new().unwrap().block_on(async {
                let tx1 = stash1
                    .transaction()
                    .await
                    .expect("Failed to start transaction");
                insert_tx(&tx1, "test1").await;
                let result = query_tx(&tx1, "test1").await;
                sleep(Duration::from_millis(100)).await;
                tx1.commit().await.expect("Failed to commit transaction");
                result
            })
        });

        // Second thread (std), with second transaction
        let handle2 = spawn(move || {
            block_on(async {
                let tx2 = stash2
                    .transaction()
                    .await
                    .expect("Failed to start transaction");
                insert_tx(&tx2, "test2").await;
                tx2.commit().await.expect("Failed to commit transaction");
                query_tx(&tx2, "test2").await
            })
        });

        // Third thread (std), with no transaction
        let handle3 = spawn(move || {
            Runtime::new().unwrap().block_on(async {
                insert(&stash3, "test3").await;
                sleep(Duration::from_millis(100)).await;
                query(&stash3, "test3").await
            })
        });

        // Fourth thread (async), with third transaction
        let handle4 = async_spawn(async move {
            let tx3 = stash4
                .transaction()
                .await
                .expect("Failed to start transaction");
            insert_tx(&tx3, "test4").await;
            let result = query_tx(&tx3, "test4").await;
            sleep(Duration::from_millis(100)).await;
            tx3.commit().await.expect("Failed to commit transaction");
            result
        });

        // Fifth thread (async), with fourth transaction
        let handle5 = async_spawn(async move {
            let tx4 = stash5
                .transaction()
                .await
                .expect("Failed to start transaction");
            insert_tx(&tx4, "test5").await;
            sleep(Duration::from_millis(100)).await;
            tx4.commit().await.expect("Failed to commit transaction");
            query_tx(&tx4, "test5").await
        });

        // Sixth thread (async), with no transaction
        let handle6 = async_spawn(async move {
            insert(&stash6, "test6").await;
            sleep(Duration::from_millis(100)).await;
            query(&stash6, "test6").await
        });

        // Wait for the threads to complete
        let result1 = handle1.join().unwrap();
        let result2 = handle2.join().unwrap();
        let result3 = handle3.join().unwrap();
        let result4 = handle4.await.unwrap();
        let result5 = handle5.await.unwrap();
        let result6 = handle6.await.unwrap();

        // Additional write queries
        stash7
            .execute(r#"INSERT INTO test_kv (value) VALUES ("test7")"#, vec![])
            .await
            .unwrap();
        insert(&stash8, "test8").await;
        let tx5 = stash9
            .transaction()
            .await
            .expect("Failed to start transaction");
        insert_tx(&tx5, "test9").await;
        let result9 = query_tx(&tx5, "test9").await;
        tx5.commit().await.expect("Failed to commit transaction");

        // Query the data, using the main Stash (no specific connection or transaction)
        let result7 = query(&stash, "test7").await;
        let result8 = stash
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

#[tokio::test]
async fn test_subscriber() {
    let db_dir = tempfile::tempdir().unwrap();
    let stash = Stash::new(Some(&db_dir.path().join("test"))).expect("Failed to create Stash");

    let subscriber = stash.subscribe().await.unwrap();

    // Create a table
    stash
        .execute(r#"CREATE TABLE test_kv (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();

    stash
        .execute(r#"CREATE TABLE test_kv2 (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();

    stash
        .execute(r#"CREATE TABLE test_kv3 (value TEXT NOT NULL)"#, vec![])
        .await
        .unwrap();

    // Insert some data without transaction
    stash
        .execute(r#"INSERT INTO test_kv3 (value) VALUES ("test")"#, vec![])
        .await
        .unwrap();

    // Start a transaction
    let tx = stash
        .transaction()
        .await
        .expect("Failed to start transaction");

    // Insert some data
    tx.execute(r#"INSERT INTO test_kv (value) VALUES ("test")"#, vec![])
        .await
        .unwrap();

    // Commit the transaction
    tx.commit().await.expect("Failed to commit transaction");

    // Start a transaction
    let tx = stash
        .transaction()
        .await
        .expect("Failed to start transaction");

    // Insert some data
    tx.execute(r#"INSERT INTO test_kv2 (value) VALUES ("test")"#, vec![])
        .await
        .unwrap();

    // Abort the transaction
    tx.rollback().await.expect("Failed to abort transaction");

    // We should receive 2 notifications , one for test_kv3 and one for test_kv
    // test_kv2 should not be triggered as the transaction was rolled back.
    let notification = subscriber.recv().unwrap();

    assert_eq!(notification.table, "test_kv3");
    assert_eq!(notification.action, Action::SQLITE_INSERT);

    let notification = subscriber.recv().unwrap();
    assert_eq!(notification.table, "test_kv");
    assert_eq!(notification.action, Action::SQLITE_INSERT);

    subscriber
        .recv_timeout(Duration::from_millis(100))
        .expect_err("Should fail");
}
