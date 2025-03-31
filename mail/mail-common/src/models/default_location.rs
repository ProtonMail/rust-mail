use std::{iter, sync::Arc, time::Instant};

use derive_more::TryFrom;
use indoc::indoc;
use proton_action_queue::queue::{ActionError as QueueActionError, Queue, QueuedActionOutput};
use proton_api_core::services::proton::Proton;

use proton_api_mail::services::proton::response_data::IncomingDefault;
use proton_api_mail::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_api_mail::{INCOMING_DEFAULTS_PAGE_SIZE, services::proton::ProtonMail};
use proton_core_common::{
    datatypes::{InitializationKey, LocalAddressId},
    models::{Address, InitializationError, InitializationWatcher, InitializedComponent},
};
use stash::{
    exports::{FromSql, FromSqlError, SqliteError, ToSql, ToSqlOutput, Value},
    params,
    stash::{Bond, Stash, StashError, Tether},
};
use tokio::task::JoinSet;
use tracing::{Level, debug};

use crate::MailContextError;
use crate::actions::addresses::block::Block;

/// Where do messages from a sender go by default. This is handled by the backend, but we sometimes
/// want this informaton for things like banners.
#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum IncomingDefaultLocation {
    /// The messages are allowed and go to inbox
    /// Email marked initially as spam by Proton, but marked as "OK" by the user.
    Inbox = 0,
    /// Marked as spam by the user, next incoming messages goes to spam directly
    Spam = 4,
    /// email address blocked by the user, going to permanent deleted immediately (not to trash, not to spam)
    /// The messages are not received and are deleted automatically
    Blocked = 14,
}

impl IncomingDefaultLocation {
    /// Finds the incoming default for a given address id.
    pub async fn find(id: LocalAddressId, tether: &Tether) -> Result<Option<Self>, StashError> {
        match tether
            .query_value::<_, Self>(
                "SELECT location AS value FROM incoming_default WHERE local_address_id = ?",
                params![id],
            )
            .await
        {
            Ok(val) => Ok(Some(val)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Stores or modifies an IncomingDefaultLocation into the database.
    pub async fn save(self, id: LocalAddressId, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            indoc! {
                "INSERT INTO incoming_default (local_address_id, location)
                VALUES (?, ?)
                ON CONFLICT(local_address_id) DO UPDATE SET
                  location = excluded.location"
            },
            params![id, self],
        )
        .await?;
        Ok(())
    }

    pub const INIT_KEY: InitializationKey = InitializationKey::new("incoming_defaults");

    /// Idempotently initialization, syncing with the backend.
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &Proton,
        stash: &Stash,
    ) -> Result<(), InitializationError<MailContextError>> {
        InitializedComponent::initialize::<MailContextError, Vec<IncomingDefault>>(
            watcher,
            Self::INIT_KEY,
            &[Address::INIT_KEY],
            stash.connection(),
            async || Self::sync(api).await,
            async |tx, res| {
                Self::store_by_email(res, tx).await?;
                Ok(())
            },
        )
        .await
    }

    /// Downloads all `IncomingDefault`s
    #[tracing::instrument(level = Level::DEBUG, skip_all)]
    async fn sync(api: &Proton) -> Result<Vec<IncomingDefault>, MailContextError> {
        let t0 = Instant::now();
        let initial = api.get_incoming_defaults(0).await?;
        debug!("Requested initial batch in {:?}", t0.elapsed());

        let mut joinset = JoinSet::new();

        let page = INCOMING_DEFAULTS_PAGE_SIZE;
        if let Some(rem) = initial.global_total.checked_sub(page) {
            let rem = rem.div_ceil(page);
            debug!("Requesting {rem} batches for contacts");
            for page in 1..=rem {
                let api = api.clone();
                joinset.spawn(async move {
                    api.get_incoming_defaults(page)
                        .await
                        .map(|x| x.incoming_defaults)
                });
            }
        }
        debug!("Requested all batches in {:?}", t0.elapsed());

        let ret = joinset.join_all().await;

        let mut out = vec![];

        for i in iter::once(Ok(initial.incoming_defaults)).chain(ret) {
            for def in i? {
                out.push(def);
            }
        }

        Ok(out)
    }

    /// Stores all `IncomingDefault`s into the database
    pub async fn store_by_email(
        items: impl IntoIterator<Item = IncomingDefault>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for def in items {
            if let (Some(email), Some(location)) = (def.email, def.location) {
                let location: IncomingDefaultLocation = location.into();
                bond.execute(
                    indoc! {"
                        INSERT OR REPLACE INTO incoming_default 
                            (local_address_id, location)
                        VALUES 
                          ( (SELECT local_id FROM addresses WHERE email = ?), ? );"
                    },
                    params![email, location],
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Stores all `IncomingDefault`s into the database
    pub async fn store_by_id(
        id: LocalAddressId,
        location: IncomingDefaultLocation,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            indoc! {"
                INSERT OR REPLACE INTO incoming_default 
                    (local_address_id, location)
                VALUES (?, ?);"
            },
            params![id, location],
        )
        .await?;
        Ok(())
    }

    /// Deletes an incoming default locally by id
    pub async fn delete_by_id(id: LocalAddressId, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM incoming_default WHERE local_address_id = ?",
            params![id],
        )
        .await?;
        Ok(())
    }

    /// Stores all `IncomingDefault`s into the database
    pub async fn delete_by_email(email: String, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            indoc! {
                "DELETE FROM incoming_default
                  WHERE local_address_id =
                   (SELECT local_id FROM addresses WHERE email = ?)"

            },
            params![email],
        )
        .await?;
        Ok(())
    }

    /// Block an address
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `address_id`  - The ID of the address to block.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_block(
        queue: &Queue,
        address_id: LocalAddressId,
    ) -> Result<QueuedActionOutput<Block>, QueueActionError<Block>> {
        let action = Block::block(address_id);
        queue.queue_action(action).await
    }

    /// Unblock an address
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `address_id`  - The ID of the address to block.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unblock(
        queue: &Queue,
        address_id: LocalAddressId,
    ) -> Result<QueuedActionOutput<Block>, QueueActionError<Block>> {
        let action = Block::unblock(address_id);
        queue.queue_action(action).await
    }
}

impl From<ApiIncomingDefaultLocation> for IncomingDefaultLocation {
    fn from(value: ApiIncomingDefaultLocation) -> Self {
        match value {
            ApiIncomingDefaultLocation::Inbox => Self::Inbox,
            ApiIncomingDefaultLocation::Spam => Self::Spam,
            ApiIncomingDefaultLocation::Blocked => Self::Blocked,
        }
    }
}

impl FromSql for IncomingDefaultLocation {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for IncomingDefaultLocation {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
