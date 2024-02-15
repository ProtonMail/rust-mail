use proton_sqlite3::rusqlite::Result;
use proton_sqlite3::{SqliteConnectionPool, SqliteMode};
use rand::prelude::*;

#[derive(Debug)]
#[allow(dead_code)]
struct Person {
    id: i32,
    name: String,
    data: Option<Vec<u8>>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Person2 {
    id: i32,
    name: String,
    data: Option<Vec<u8>>,
    data2: Option<Vec<u8>>,
}

fn run_tasks(pool: SqliteConnectionPool, count: usize) -> Result<()> {
    let mut conn = pool.acquire()?;
    let mut rng = thread_rng();

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
                let mut stmt = conn.prepare("SELECT person.id, person.name, person.data , person_map.data2 FROM person JOIN person_map ON person.id=person_map.person")?;
                let person_iter = stmt.query_map([], |row| {
                    Ok(Person2 {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        data: row.get(2)?,
                        data2: row.get(3)?,
                    })
                })?;
                let _count = person_iter.into_iter().count();
            }
            1 => {
                conn.tx(|tx| -> Result<()> {
                    let mut nums: Vec<u8> = (1..20).collect();
                    nums.shuffle(&mut rng);
                    let me = Person {
                        id: 0,
                        name: format!("{:?}", nums),
                        data: Some(nums.clone()),
                    };

                    let id: i32 = tx.query_row(
                        "INSERT INTO person (name, data) VALUES (?1, ?2) RETURNING `id`",
                        (&me.name, &me.data),
                        |row| row.get(0),
                    )?;

                    nums.shuffle(&mut rng);

                    tx.execute(
                        "INSERT INTO person_map (person, data2) VALUES (?1, ?2)",
                        (id, format!("{:?}", nums)),
                    )?;
                    Ok(())
                })?;
            }

            2 => {
                conn.tx(|tx| -> Result<()> {
                    tx.execute(
                        "DELETE FROM person WHERE id=(SELECT id FROM person ORDER BY RANDOM() LIMIT 1)",
                        (),
                    )?;
                    Ok(())
                })?;
            }
            _ => {
                panic!("Unhandled");
            }
        }
    }

    Ok(())
}

#[test]
fn run_random_command_test() {
    // Simple execution test that spawns a couple of threads that executes a create, select or delete
    // command at random.
    let dir = tempdir::TempDir::new("sqlite3_test").expect("failed to create temp dir");
    let db_path = dir.path().join("sqlite.db");

    let connection_pool = SqliteConnectionPool::new(SqliteMode::File(db_path));

    {
        // Create db tables.
        let conn = connection_pool
            .acquire()
            .expect("Failed to acquire connection");

        conn.execute(
            "CREATE TABLE person (
            id   INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            data BLOB
        )",
            (), // empty list of parameters.
        )
        .expect("failed to run create query");

        conn.execute(
            "CREATE TABLE person_map (
            person INTEGER,
            data2 TEXT NOT NULL,
            CONSTRAINT `person_ref` FOREIGN KEY (`person`) REFERENCES `person` (`id`) ON DELETE SET NULL
        )",
            (), // empty list of parameters.
        ).expect("failed to run create query");
    }

    let rdonly = connection_pool
        .acquire_read_only()
        .expect("failed to acquire read only");

    let _watcher = connection_pool.watch(move |event| match event {
        Err(e) => {
            eprintln!("failed to watch: {e}");
            return;
        }
        Ok(_) => {
            let version = rdonly.data_version().expect("failed to get version");
            println!("Change detected {version}");
        }
    });

    let mut handles = Vec::new();
    for _ in 0..10 {
        let pool = connection_pool.clone();
        handles.push(std::thread::spawn(move || {
            run_tasks(pool, 100).expect("Failed to execute");
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    connection_pool
        .close_all()
        .expect("Failed to close all connections");
}
