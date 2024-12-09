#![allow(non_snake_case)]

use rusqlite::hooks::Action;
use stash::stash::Stash;

#[tokio::test]
async fn test_service() {
    let orig = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig(panic_info);
        std::process::exit(-1);
    }));
    let stash = Stash::new(None).expect("Failed to create Stash");
    let mut conn = stash.connection();

    let tx = conn
        .transaction()
        .await
        .expect("Failed to start transaction");
    tx.execute(
        "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)",
        vec![],
    )
    .await
    .unwrap();
    tx.execute("CREATE TABLE bar (v INTEGER UNIQUE)", vec![])
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let subscriber = stash.subscribe().await.expect("Failed to subscribe");

    conn.execute("INSERT INTO foo VALUES( null,10)", vec![])
        .await
        .unwrap();
    let notification = subscriber.recv_async().await.unwrap();
    assert_eq!(notification.action, Action::SQLITE_INSERT);
    assert_eq!(notification.table, "foo".to_owned());
    assert_eq!(notification.row, 1);

    conn.execute("INSERT OR REPLACE INTO bar VALUES(10)", vec![])
        .await
        .unwrap();
    let notification = subscriber.recv_async().await.unwrap();
    assert_eq!(notification.action, Action::SQLITE_INSERT);
    assert_eq!(notification.table, "bar".to_owned());
    assert_eq!(notification.row, 1);

    conn.execute("INSERT OR REPLACE INTO bar VALUES(10)", vec![])
        .await
        .unwrap();
    let notification = subscriber.recv_async().await.unwrap();
    assert_eq!(notification.action, Action::SQLITE_INSERT);
    assert_eq!(notification.table, "bar".to_owned());
    assert_eq!(notification.row, 2);

    let tx = conn.transaction().await.unwrap();
    tx.execute("DELETE FROM foo WHERE v=10", vec![])
        .await
        .unwrap();
    tx.execute("DELETE FROM bar WHERE v=10", vec![])
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let notification = subscriber.recv_async().await.unwrap();
    assert_eq!(notification.action, Action::SQLITE_DELETE);
    assert_eq!(notification.table, "foo".to_owned());
    assert_eq!(notification.row, 1);
    let notification = subscriber.recv_async().await.unwrap();
    assert_eq!(notification.action, Action::SQLITE_DELETE);
    assert_eq!(notification.table, "bar".to_owned());
    assert_eq!(notification.row, 2);
}
