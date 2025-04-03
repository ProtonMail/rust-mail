#![allow(dead_code)]

use proton_action_queue::action::{Action, Factory};
use proton_action_queue::queue::Queue;
#[allow(unused_imports)]
pub use proton_action_queue::tests::common::DefaultError;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::{Bond, Stash, StashConfiguration, StashError, Tether};

/// Create a new queue.
pub async fn new_queue(factory: Factory) -> Queue {
    Queue::with_factory(new_stash().await, factory)
        .await
        .unwrap()
}

/// Create a new queue with a given db `pool`.
pub async fn new_queue_with_stash(stash: Stash, factory: Factory) -> Queue {
    Queue::with_factory(stash, factory).await.unwrap()
}

pub async fn new_stash() -> Stash {
    let stash = Stash::new(StashConfiguration::test()).unwrap();
    let mut conn = stash.connection();
    conn.tx(async |tx| tx.ext_create_table().await)
        .await
        .unwrap();
    stash
}

pub async fn new_queue_typed<T: Action<Context: Default>>() -> Queue {
    new_queue(new_factory::<T>()).await
}

/// Create a new factory with an action.
pub fn new_factory<T: Action>() -> Factory {
    let mut factory = Factory::new();
    factory.register::<T>().unwrap();
    factory
}

pub trait TestReadExtension {
    async fn ext_get_value(&self, key: &str) -> Result<Option<u32>, StashError>;
}

pub trait TestWriteExtension: TestReadExtension {
    async fn ext_create_table(&self) -> Result<(), StashError>;

    async fn ext_insert_value(&self, key: &str, value: u32) -> Result<(), StashError>;

    async fn ext_delete_value(&self, key: &str) -> Result<(), StashError>;
}

impl TestReadExtension for Tether {
    async fn ext_get_value(&self, key: &str) -> Result<Option<u32>, StashError> {
        match self
            .query_value::<_, u32>(
                "SELECT value FROM ext WHERE key = ?",
                params![key.to_owned()],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}

impl TestReadExtension for Bond<'_> {
    async fn ext_get_value(&self, key: &str) -> Result<Option<u32>, StashError> {
        match self
            .query_value::<_, u32>(
                "SELECT value FROM ext WHERE key = ?",
                params![key.to_owned()],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}

impl TestWriteExtension for Bond<'_> {
    async fn ext_create_table(&self) -> Result<(), StashError> {
        self.execute(
            "CREATE TABLE ext (key TEXT PRIMARY KEY, value INTEGER NOT NULL)",
            vec![],
        )
        .await?;
        Ok(())
    }

    async fn ext_insert_value(&self, key: &str, value: u32) -> Result<(), StashError> {
        self.execute(
            "INSERT OR REPLACE INTO ext VALUES (?,?)",
            params![key.to_owned(), value],
        )
        .await?;
        Ok(())
    }

    async fn ext_delete_value(&self, key: &str) -> Result<(), StashError> {
        self.execute("DELETE FROM ext WHERE key=?", params![key.to_owned()])
            .await?;
        Ok(())
    }
}
