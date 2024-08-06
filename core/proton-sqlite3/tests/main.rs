use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use stash::datatypes::QueryResultI64;
use stash::macros::DbRecord;
use stash::params;
use stash::stash::{Interface, Stash, StashError};
use tokio::spawn as spawn_async;

#[derive(Debug)]
#[allow(dead_code)]
struct Person {
    id: i32,
    name: String,
    data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, DbRecord, PartialEq)]
#[allow(dead_code)]
struct Person2 {
    #[DbField]
    id: i32,
    #[DbField]
    name: String,
    #[DbField]
    data: Option<Vec<u8>>,
    #[DbField]
    data2: String,
}

async fn run_tasks(stash: Stash, count: usize) -> Result<(), StashError> {
    let conn = stash.connection();
    let mut rng = StdRng::from_entropy();

    //conn.busy_timeout(Duration::from_secs(10))?;

    // Enable for Sqlite Hooks
    /*
    let thread_id = std::thread::current().id();
    conn.update_hook(Some(move |action, db: &str, table: &str, row_id| {
        match action {
            Action::UNKNOWN => {
                println!("[{:?}][{db}:{table}] Unknown action", thread_id);
            }
            Action::SQLITE_DELETE => {
                println!("[{:?}][{db}:{table}] delete {row_id}", thread_id);
            }
            Action::SQLITE_INSERT => {
                println!("[{:?}][{db}:{table}] insert {row_id}", thread_id);
            }
            Action::SQLITE_UPDATE => {
                println!("[{:?}][{db}:{table}] update {row_id}", thread_id);
            }
            _ => {
                println!("[{:?}][{db}:{table}] Other???", thread_id);
            }
        };
    }));*/

    for _ in 0..count {
        match rng.gen::<u32>() % 3 {
            0 => {
                conn.query::<_, Person2>("SELECT person.id, person.name, person.data , person_map.data2 FROM person JOIN person_map ON person.id=person_map.person", vec![]).await?;
            }
            1 => {
                conn.transaction().await?;
                let mut nums: Vec<u8> = (1..20).collect();
                nums.shuffle(&mut rng);
                let me = Person {
                    id: 0,
                    name: format!("{:?}", nums),
                    data: Some(nums.clone()),
                };

                let id: i32 = conn
                    .query::<_, QueryResultI64>(
                        "INSERT INTO person (name, data) VALUES (?1, ?2) RETURNING `id` AS value",
                        params![me.name, me.data],
                    )
                    .await?
                    .first()
                    .unwrap()
                    .value as i32;

                nums.shuffle(&mut rng);

                conn.execute(
                    "INSERT INTO person_map (person, data2) VALUES (?1, ?2)",
                    params![id, format!("{:?}", nums)],
                )
                .await?;
                conn.commit().await.unwrap();
            }

            2 => {
                conn.execute(
                    "DELETE FROM person WHERE id=(SELECT id FROM person ORDER BY RANDOM() LIMIT 1)",
                    vec![],
                )
                .await?;
            }
            _ => {
                panic!("Unhandled");
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn run_random_command_test() {
    // Simple execution test that spawns a couple of threads that executes a create, select or delete
    // command at random.
    let dir = tempdir::TempDir::new("sqlite3_test").expect("failed to create temp dir");
    let db_path = dir.path().join("sqlite.db");

    let stash = Stash::new(Some(&db_path)).expect("Failed to create Stash");

    // Create db tables.
    stash
        .execute(
            "CREATE TABLE person (
            id   INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            data BLOB
        )",
            vec![], // empty list of parameters.
        )
        .await
        .expect("failed to run create query");

    stash.execute(
            "CREATE TABLE person_map (
            person INTEGER,
            data2 TEXT NOT NULL,
            CONSTRAINT `person_ref` FOREIGN KEY (`person`) REFERENCES `person` (`id`) ON DELETE SET NULL
        )",
            vec![], // empty list of parameters.
        ).await.expect("failed to run create query");

    let watcher = stash.subscribe().await.expect("Failed to watch");
    let _watcher_handle = spawn_async(async move {
        while let Ok(notification) = watcher.recv_async().await {
            println!("Change detected {:?}", notification);
        }
    });

    let mut handles = Vec::new();
    for _ in 0..10 {
        let stash_clone = stash.clone();
        handles.push(spawn_async(async move {
            run_tasks(stash_clone, 100)
                .await
                .expect("Failed to execute");
        }));
    }

    for h in handles {
        h.await.expect("thread panicked");
    }
}
