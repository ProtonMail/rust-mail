use std::{iter, sync::Arc, time::Instant};

use derive_more::TryFrom;
use indoc::indoc;
use proton_action_queue::queue::{ActionError as QueueActionError, Queue, QueuedActionOutput};
use proton_core_api::service::ApiServiceResult;
use proton_core_api::services::proton::PrivateEmail;

use proton_core_api::session::Session;
use proton_core_common::{
    datatypes::InitializationKey,
    models::{Address, InitializationError, InitializationWatcher, InitializedComponent},
};
use proton_mail_api::services::proton::response_data::IncomingDefault;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_mail_api::{INCOMING_DEFAULTS_PAGE_SIZE, services::proton::ProtonMail};
use stash::{
    exports::{FromSql, FromSqlError, SqliteError, ToSql, ToSqlOutput, Value},
    params,
    stash::{Bond, Stash, StashError, Tether},
};
use tokio::task::JoinSet;
use tracing::debug;
use tracing::error;

use crate::MailContextError;
use crate::actions::addresses::block::Block;
use crate::actions::addresses::unblock::Unblock;
use crate::actions::addresses::update_incoming_defaults::SyncIncomingDefaults;

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
    pub async fn find(email: String, tether: &Tether) -> Result<Option<Self>, StashError> {
        match tether
            .query_value::<_, Self>(
                "SELECT location AS value FROM incoming_default WHERE email = ?",
                params![email],
            )
            .await
        {
            Ok(val) => Ok(Some(val)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub const INIT_KEY: InitializationKey = InitializationKey::new("incoming_defaults");

    /// Idempotently initialization, syncing with the backend.
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &Session,
        stash: &Stash,
    ) -> Result<(), InitializationError<MailContextError>> {
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[Address::INIT_KEY],
            stash.connection(),
            async || Ok(Self::sync(api).await?),
            async |tx, res| {
                Self::store_by_email(res, tx).await?;
                Ok(())
            },
        )
        .await
    }

    /// Downloads all `IncomingDefault`s
    #[tracing::instrument(skip_all)]
    pub async fn sync(api: &Session) -> ApiServiceResult<Vec<IncomingDefault>> {
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

        for defs in iter::once(Ok(initial.incoming_defaults)).chain(ret) {
            for def in defs? {
                out.push(def);
            }
        }

        Ok(out)
    }

    /// Stores all `IncomingDefault`s into the database
    pub async fn store_by_email(
        defs: impl IntoIterator<Item = IncomingDefault>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for def in defs {
            let location = def.location.map(IncomingDefaultLocation::from);

            bond.execute(
                indoc! {"
                        INSERT INTO incoming_default 
                            (email, location, id, domain)
                        VALUES 
                          (?,?,?,?);"
                },
                params![def.email, location, def.id, def.domain],
            )
            .await?;
        }
        Ok(())
    }

    /// Stores all `IncomingDefault`s into the database
    pub async fn delete_by_email(email: String, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            "DELETE FROM incoming_default WHERE email = ?",
            params![email],
        )
        .await?;
        Ok(())
    }

    /// Blocks an address
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_block(
        queue: &Queue,
        email: PrivateEmail,
    ) -> Result<QueuedActionOutput<Block>, QueueActionError<Block>> {
        let action = Block { email };
        queue.queue_action(action).await
    }

    /// Unblocks an address
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unblock(
        queue: &Queue,
        email: PrivateEmail,
    ) -> Result<QueuedActionOutput<Unblock>, QueueActionError<Unblock>> {
        let action = Unblock { email };
        queue.queue_action(action).await
    }

    /// Reloads the data from the API.
    pub async fn action_resync(queue: &Queue) {
        if let Err(e) = queue.queue_action(SyncIncomingDefaults).await {
            if cfg!(debug_assertions) {
                panic!("apply_local can't fail {e}");
            } else {
                error!(?e);
            }
        }
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
