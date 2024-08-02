#![allow(dead_code)]
use proton_action_queue::action::{Action, Error, Factory};
use proton_action_queue::db::{ActionQueueExtension, OptionalExtension};
use proton_action_queue::queue::Queue;
use proton_api_core::service::ApiServiceError;
use proton_api_core::session::Session;
use stash::macros::DbRecord;
use stash::params;
use stash::stash::{Interface, Stash, StashError, Tether};

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
    let stash = Stash::new(None).unwrap();
    let tx = stash.transaction().await.unwrap();
    tx.ext_create_table().await.unwrap();
    tx.commit().await.unwrap();
    stash
}

pub async fn new_queue_typed<T: Action>() -> Queue {
    new_queue(new_factory::<T>()).await
}

/// Create a new test session with bogus values.
pub async fn new_session() -> Session {
    let config = proton_api_core::services::proton::Config {
        app_version: "TEST".to_owned(),
        base_url: "https://test.com".to_owned(),
        user_agent: "TEST".to_owned(),
        allow_http: true,
        skip_srp_proof_validation: true,
    };
    Session::new(config, None).await.unwrap()
}

/// Create a new factory with an action.
pub fn new_factory<T: Action>() -> Factory {
    let mut factory = Factory::new();
    factory.register::<T>().unwrap();
    factory
}

pub trait TestExtension {
    async fn ext_create_table(&self) -> Result<(), StashError>;

    async fn ext_insert_value(&self, key: &str, value: u32) -> Result<(), StashError>;

    async fn ext_delete_value(&self, key: &str) -> Result<(), StashError>;

    async fn ext_get_value(&self, key: &str) -> Result<Option<u32>, StashError>;
}

impl TestExtension for Tether {
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

    async fn ext_get_value(&self, key: &str) -> Result<Option<u32>, StashError> {
        #[derive(Debug, Copy, Clone, Eq, PartialEq, DbRecord)]
        struct Record {
            #[DbField]
            value: u32,
        }
        let v = self
            .query_row::<_, Record>("SELECT value FROM ext WHERE key=?", params![key.to_owned()])
            .await
            .optional()?;

        Ok(v.map(|v| v.value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DefaultError {
    #[error("{0}")]
    Request(#[from] ApiServiceError),
    #[error("{0}")]
    Other(anyhow::Error),
    #[error("{0}")]
    DB(#[from] StashError),
}

impl Error for DefaultError {
    fn request_error(&self) -> Option<&ApiServiceError> {
        let Self::Request(err) = self else {
            return None;
        };

        Some(err)
    }
}
