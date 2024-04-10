use proton_sqlite3::utils::mapped_rows_to_vec;
use proton_sqlite3::{
    InProcessTrackerService, LiveQueryBuilder, Observable, SqliteConnection, SqliteConnectionPool,
    SqliteMode, TrackingConnection,
};
use std::ops::Deref;
use std::sync::mpsc::TryRecvError;

#[derive(Clone)]
struct TestQuery {}

impl Observable for TestQuery {
    type Output = Vec<(i64, i64)>;

    fn debug_name(&self) -> &'static str {
        "test query"
    }

    fn tables(&self) -> Vec<String> {
        vec!["foo".into()]
    }

    fn execute(&self, connection: &SqliteConnection) -> rusqlite::Result<Self::Output> {
        let mut stmt = connection.prepare("SELECT * FROM foo")?;
        let x = mapped_rows_to_vec(stmt.query_map((), |r| -> rusqlite::Result<(i64, i64)> {
            Ok((r.get(0)?, r.get(1)?))
        })?)?;
        Ok(x)
    }
}

#[test]
fn test_tracker() {
    let dir = tempdir::TempDir::new("sqlite3_test").expect("failed to create temp dir");
    let db_path = dir.path().join("sqlite.db");
    let connection_pool = SqliteConnectionPool::new(SqliteMode::File(db_path), false);
    {
        let mut connection = connection_pool
            .acquire()
            .expect("failed to acquire connection");

        connection
            .tx(|tx| -> rusqlite::Result<()> {
                tx.execute(
                    "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, bar INTEGER)",
                    (),
                )?;
                Ok(())
            })
            .expect("failed to create table");
    }

    let tracker_service = InProcessTrackerService::new(connection_pool.clone()).unwrap();

    let live_query = LiveQueryBuilder::new(tracker_service.clone()).build(TestQuery {});

    println!(">> {:?}", live_query.value().deref());
    let (sender, receiver) = std::sync::mpsc::sync_channel::<()>(0);

    let mut join_handles = Vec::new();
    for _ in 0..3 {
        let connection_pool = connection_pool.clone();
        let tracker_service = tracker_service.clone();
        let h = std::thread::spawn(move || {
            let connection = connection_pool
                .acquire()
                .expect("failed to acquire connection");

            let mut tracking_connection =
                TrackingConnection::new(connection, tracker_service.clone())
                    .expect("Failed to init tracker");

            tracking_connection
                .tx(|tx| -> rusqlite::Result<()> {
                    tx.execute("INSERT INTO foo VALUES (null, 10)", ())?;
                    Ok(())
                })
                .expect("failed tx");
        });

        join_handles.push(h);
    }

    std::thread::spawn(move || {
        for (i, h) in join_handles.into_iter().enumerate() {
            h.join().expect(&format!("failed to join thread {i}"));
        }
        sender.send(())
    });

    loop {
        match receiver.try_recv() {
            Ok(_) => {
                println!(">> {:?}", live_query.value().deref())
            }
            Err(e) => match e {
                TryRecvError::Empty => continue,
                TryRecvError::Disconnected => break,
            },
        }
    }
    println!(">> {:?}", live_query.value().deref())
}
