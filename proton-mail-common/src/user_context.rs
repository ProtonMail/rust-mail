mod action_queue;
pub mod cache;
mod events;
mod images;
mod initialization;

use crate::models::{Conversation, Message};
use crate::user_context::action_queue::new_action_queue;
use crate::user_context::cache::{Cache, CacheAttachmentConfig, CacheMessageConfig};
use crate::{MailContext, MailContextError, MailContextResult};
use anyhow::anyhow;
use futures::executor::block_on;
pub use initialization::*;
use proton_action_queue::queue::Queue;
use proton_api_core::auth::UserKeySecret;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::cache::ProtonCache;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::models::User;
use proton_core_common::{LoadKeySecret, UserContext};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto_account::keys::{UnlockedAddressKeys, UnlockedUserKeys};
use proton_event_loop::foreground_loop::EventLoop;
use stash::orm::Model;
use stash::stash::Stash;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tracing::error;

pub struct MailUserContext {
    this: Weak<Self>,
    mail_context: MailContext,
    user_context: UserContext,
    event_loop: EventLoop,
    action_queue: Queue,
    cache: Cache,
}

impl MailUserContext {
    pub(crate) async fn new(
        mail_context: MailContext,
        user_context: UserContext,
    ) -> MailContextResult<Arc<Self>> {
        let stash = user_context.stash().clone();
        let cache_path = mail_context.mail_cache_path(user_context.user_id());
        let cache = Cache::new(cache_path, mail_context.mail_cache_size)?;
        let action_queue = new_action_queue(stash).await.unwrap();
        let this = Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            mail_context,
            user_context,
            event_loop: EventLoop::new(),
            action_queue,
            cache,
        });

        this.init_expiration_loop();
        Ok(this)
    }

    /// Sets a background job where every 60 seconds it deletes all of the messages and conversations
    /// that have an expiration date.
    fn init_expiration_loop(&self) {
        let db = self.user_stash().into();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                if let Err(e) = Conversation::delete_expired(&db).await {
                    error!("Error in background task deleting expired conversations: {e}");
                }

                if let Err(e) = Message::delete_expired(&db).await {
                    error!("Error in background task deleting expired messages: {e}");
                }
                interval.tick().await;
            }
        });
    }

    /// Return a reference on the attachments cache
    pub fn attachements_cache(&self) -> &ProtonCache<CacheAttachmentConfig> {
        &self.cache.attachments_cache
    }

    /// Return a reference on the message body cache
    pub fn messages_cache(&self) -> &ProtonCache<CacheMessageConfig> {
        &self.cache.messages_cache
    }

    pub fn session(&self) -> &Session {
        self.user_context.session()
    }

    pub fn queue(&self) -> &Queue {
        &self.action_queue
    }

    /// Get the API service.
    pub fn api(&self) -> &Proton {
        self.user_context.session().api()
    }

    /// Get the database connection.
    #[must_use]
    pub fn user_stash(&self) -> &Stash {
        self.user_context.stash()
    }

    pub fn mail_context(&self) -> &MailContext {
        &self.mail_context
    }

    pub fn user_context(&self) -> &UserContext {
        &self.user_context
    }

    pub fn user_id(&self) -> &RemoteId {
        self.user_context.user_id()
    }

    /// Provides a way to get the core::models::User instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> MailContextResult<User> {
        let stash = self.user_stash();
        let user_id = self.user_id();
        let real_user = User::load(user_id.clone(), stash)
            .await?
            .ok_or_else(|| MailContextError::Other(anyhow!("Missing User, this is a bug.")))?;

        Ok(real_user)
    }

    /// Returns the unlocked user keys of this user.
    ///
    /// # Warning
    /// Cannot be called from an async context as it uses the runtime to block.
    /// Use [`MailUserContext::user_keys_unlocked_async`] instead.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn unlocked_user_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> MailContextResult<UnlockedUserKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_user_keys(pgp_provider, self)
            .await?;
        Ok(keys)
    }

    /// Returns the unlocked user keys of this user from an async context..
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn unlocked_user_keys_async<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
    ) -> MailContextResult<UnlockedUserKeys<Provider>> {
        let secret_loader = CloneSecretLoader(self.session().expose_key_secret().await);
        let keys = self
            .user_context
            .unlocked_user_keys(pgp_provider, &secret_loader)
            .await?;
        Ok(keys)
    }

    /// Returns the unlocked address keys for this user.
    ///
    /// # Warning
    /// Cannot be called from an async context as it uses the runtime to block.
    /// Use [`MailUserContext::address_keys_unlocked_async`] instead.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn unlocked_address_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        address_id: &RemoteId,
    ) -> MailContextResult<UnlockedAddressKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_address_keys(pgp_provider, self, address_id)
            .await?;
        Ok(keys)
    }

    /// Returns the unlocked address keys for this user from an async context.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn unlocked_address_keys_async<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        address_id: &RemoteId,
    ) -> MailContextResult<UnlockedAddressKeys<Provider>> {
        // TODO: This should not be necessary and handled by the UserContext
        let secret = CloneSecretLoader(self.session().expose_key_secret().await);
        let keys = self
            .user_context
            .unlocked_address_keys(pgp_provider, &secret, address_id)
            .await?;
        Ok(keys)
    }

    /// Returns the cache path for mail related resource.
    pub fn mail_cache_path(&self) -> PathBuf {
        self.mail_context.mail_cache_path(self.user_id())
    }

    pub async fn logout(&self) -> MailContextResult<()> {
        self.user_context.session().logout().await?;
        Ok(())
    }

    /// Ping the proton servers to see if they are responsive/alive.
    pub async fn ping(&self) -> MailContextResult<()> {
        self.user_context.session().api().get_tests_ping().await?;
        Ok(())
    }
}

struct CloneSecretLoader(Option<UserKeySecret>);

impl LoadKeySecret for CloneSecretLoader {
    fn key_secret(&self) -> Option<UserKeySecret> {
        self.0.clone()
    }
}

impl LoadKeySecret for MailUserContext {
    fn key_secret(&self) -> Option<UserKeySecret> {
        block_on(self.session().expose_key_secret())
    }
}
