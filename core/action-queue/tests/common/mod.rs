#![allow(dead_code)]
use std::sync::Arc;

use proton_action_queue::action::{Action, Error, Factory, WriterGuardError};
use proton_action_queue::network::WaitForOnline;
use proton_action_queue::queue::Queue;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::{Bond, Stash, StashError, Tether};

pub(crate) struct DummyWaitForOnline;
#[async_trait::async_trait]
impl WaitForOnline for DummyWaitForOnline {
    async fn wait_for_online(&self) {}
}

/// Create a new queue.
pub async fn new_queue(factory: Factory) -> Queue {
    Queue::with_factory(new_stash().await, factory, Arc::new(DummyWaitForOnline))
        .await
        .unwrap()
}

/// Create a new queue with a given db `pool`.
pub async fn new_queue_with_stash(stash: Stash, factory: Factory) -> Queue {
    Queue::with_factory(stash, factory, Arc::new(DummyWaitForOnline))
        .await
        .unwrap()
}

/// Create a new queue with given mechanism for waiting for online status
///
pub async fn new_queue_with_custom_network_waiting(
    factory: Factory,
    wait_for_online: impl WaitForOnline + 'static,
) -> Queue {
    Queue::with_factory(new_stash().await, factory, Arc::new(wait_for_online))
        .await
        .unwrap()
}

pub async fn new_stash() -> Stash {
    let stash = Stash::new(None).unwrap();
    let mut conn = stash.connection();
    let tx = conn.transaction().await.unwrap();
    tx.ext_create_table().await.unwrap();
    tx.commit().await.unwrap();
    stash
}

pub async fn new_queue_typed<T: Action<Context: Default>>() -> Queue {
    new_queue(new_factory::<T>()).await
}

pub async fn new_queue_typed_with_custom_network_waiting<T: Action<Context: Default>>(
    wait_for_online: impl WaitForOnline + 'static,
) -> Queue {
    new_queue_with_custom_network_waiting(new_factory::<T>(), wait_for_online).await
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

#[derive(Debug, thiserror::Error)]
pub enum DefaultError {
    #[error("Network Failure")]
    NetworkFailure,
    #[error("API Failure")]
    APIFailure,
    #[error("{0}")]
    Other(anyhow::Error),
    #[error("{0}")]
    DB(#[from] StashError),
    #[error("Writer Guard Expired")]
    WriterGuardExpired,
}

impl From<WriterGuardError> for DefaultError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::WriterGuardExpired,
            WriterGuardError::Stash(e) => Self::DB(e),
        }
    }
}

impl Error for DefaultError {
    fn is_network_failure(&self) -> bool {
        matches!(self, DefaultError::NetworkFailure)
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, DefaultError::WriterGuardExpired)
    }
}
