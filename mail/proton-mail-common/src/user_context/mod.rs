mod action_queue;
mod conversations;
mod events;
mod images;
mod initialization;
mod labels;
mod settings;

pub use initialization::*;
use proton_action_queue::ActionQueue;
use proton_api_mail::proton_api_core::auth::UserKeySecret;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_account::domain::{UnlockedAddressKeys, UnlockedUserKeys};
use std::sync::{Arc, Weak};

use crate::db::{
    new_mail_settings_live_query, MailSettingsLiveQuery, MailSqliteConnection,
    MailSqliteConnectionMut, MailSqliteConnectionRef,
};
use crate::user_context::action_queue::new_action_queue;
use crate::{MailContext, MailContextResult};
use proton_api_mail::proton_api_core::domain::{AddressId, UserId};
use proton_api_mail::proton_api_core::exports::proton_sqlite3::InProcessTrackerService;
use proton_api_mail::proton_api_core::Session;
use proton_api_mail::MailSession;
use proton_core_common::db::DBResult;
use proton_core_common::{LoadKeySecret, UserContext};
use proton_event_loop::EventLoop;

#[derive(Clone)]
pub struct MailUserContext {
    inner: Arc<MailUserContextInner>,
}

#[derive(Debug, Clone)]
pub struct WeakMailUserContext {
    inner: Weak<MailUserContextInner>,
}

struct MailUserContextInner {
    mail_context: MailContext,
    user_context: UserContext,
    event_loop: EventLoop,
    action_queue: ActionQueue,
    mail_settings: MailSettingsLiveQuery,
}

impl WeakMailUserContext {
    pub(crate) fn new(ctx: &MailUserContext) -> Self {
        Self {
            inner: Arc::downgrade(&ctx.inner),
        }
    }
    pub fn upgrade(&self) -> Option<MailUserContext> {
        self.inner.upgrade().map(|v| MailUserContext { inner: v })
    }
}

impl From<MailUserContext> for WeakMailUserContext {
    fn from(value: MailUserContext) -> Self {
        Self {
            inner: Arc::downgrade(&value.inner),
        }
    }
}

impl MailUserContext {
    pub(crate) fn new(mail_context: MailContext, user_context: UserContext) -> Self {
        let mail_settings = new_mail_settings_live_query(user_context.tracker_service().clone());
        Self {
            inner: Arc::new_cyclic(|weak| MailUserContextInner {
                user_context,
                mail_context,
                event_loop: EventLoop::new(),
                action_queue: new_action_queue(WeakMailUserContext {
                    inner: weak.clone(),
                }),
                mail_settings,
            }),
        }
    }

    pub(crate) fn session(&self) -> &Session {
        self.inner.user_context.session()
    }

    pub(crate) fn mail_session(&self) -> MailSession {
        self.inner.user_context.session_as::<MailSession>()
    }

    pub(crate) fn new_db_connection(&self) -> DBResult<MailSqliteConnection> {
        self.inner
            .user_context
            .new_db_connection_as::<MailSqliteConnection>()
    }

    pub(crate) fn tracker_service(&self) -> &InProcessTrackerService {
        self.inner.user_context.tracker_service()
    }

    /// Read from the user database.
    ///
    /// # Errors
    /// Returns error if we failed to acquire a connection or the read closure returned error.
    pub fn db_read<R, E, F>(&self, f: F) -> Result<R, E>
    where
        E: From<proton_sqlite3::rusqlite::Error>,
        F: FnMut(&MailSqliteConnectionRef) -> Result<R, E>,
    {
        let conn = self.new_db_connection()?;
        conn.read(f)
    }

    // TODO: this currently cant be enabled to due to incorrect api in the proton-sqlite3 crate.
    /*/// Write on the user database in a transaction from an asynchronous context.
    ///
    /// # Errors
    /// Returns error if we failed to acquire a connection, the closure return and error or the
    /// transaction failed to commit.
    pub async fn db_write_async<R,E,F>(&self, mut f:F) -> Result<R,E> where
        R: Send + 'static,
        E: From<proton_sqlite3::rusqlite::Error> + Send + 'static,
        F: FnMut(&mut MailSqliteConnectionMut) -> Result<R,E> + Send + 'static{
        self.tracker_service().new_connection_async(move |tx| {
            let mut tx = MailSqliteConnectionMut::new(tx);
            f(&mut tx)
        }).await
    }*/

    /// Perform a write on the user database in a transaction.
    ///
    /// # Errors
    /// Returns error if we failed to acquire a connection, the closure return and error or the
    /// transaction failed to commit.
    pub fn db_write<R, E, F>(&self, f: F) -> Result<R, E>
    where
        E: From<proton_sqlite3::rusqlite::Error>,
        F: FnMut(&mut MailSqliteConnectionMut) -> Result<R, E>,
    {
        let mut conn = self.new_db_connection()?;
        conn.tx(f)
    }

    pub fn mail_context(&self) -> &MailContext {
        &self.inner.mail_context
    }

    pub fn user_id(&self) -> &UserId {
        self.inner.user_context.user_id()
    }

    /// Returns the unlocked user keys of this user.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub fn user_keys_unlocked<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> MailContextResult<UnlockedUserKeys<Provider>> {
        let keys = self
            .inner
            .user_context
            .user_keys_unlocked(pgp_provider, self)?;
        Ok(keys)
    }

    /// Returns the unlocked address keys for this user.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub fn address_keys_unlocked<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        address_id: &AddressId,
    ) -> MailContextResult<UnlockedAddressKeys<Provider>> {
        let keys = self
            .inner
            .user_context
            .address_keys_unlocked(pgp_provider, self, address_id)?;
        Ok(keys)
    }

    pub async fn logout(&self) -> MailContextResult<()> {
        self.inner.user_context.session().logout().await?;
        Ok(())
    }

    /// Ping the proton servers to see if they are responsive/alive.
    pub async fn ping(&self) -> MailContextResult<()> {
        self.inner.user_context.session().ping().await?;
        Ok(())
    }
}

impl LoadKeySecret for MailUserContext {
    fn key_secret(&self) -> Option<UserKeySecret> {
        self.mail_context()
            .async_runtime()
            .block_on(self.session().expose_key_secret())
    }
}
