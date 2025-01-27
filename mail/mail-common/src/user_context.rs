mod action_queue;
pub mod cache;
mod events;
pub mod exclusive_updates;
mod images;
mod initialization;

use crate::models::{Conversation, Message};
use crate::user_context::action_queue::new_action_queue;
use crate::user_context::cache::{Cache, CacheAttachmentConfig, CacheMessageConfig};
use crate::user_context::exclusive_updates::MailUserContextExclusive;
use crate::{AppError, MailContext, MailContextError, MailContextResult};
use anyhow::anyhow;
pub use initialization::*;
use proton_action_queue::queue::{Queue, QueuedResult};
use proton_api_core::auth::UserKeySecret;
use proton_api_core::crypto_clock;
use proton_api_core::services::proton::common::{AddressId, AuthId, UserId};
use proton_api_core::services::proton::{Proton, ProtonCore};
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::cache::ProtonCache;
use proton_core_common::datatypes::{AccountDetails, ConnectionStatus, LocalAddressId};
use proton_core_common::models::{Address, User};
use proton_core_common::{ContactError, CoreContextError, LoadKeySecret, UserContext};
use proton_crypto_inbox::keys::{ComposerPreference, CryptoMailSettings, SendPreferences};
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto::CryptoClockProvider;
use proton_crypto_inbox::proton_crypto_account::keys::{UnlockedAddressKeys, UnlockedUserKeys};
use proton_event_loop::foreground_loop::EventLoop;
use stash::orm::Model;
use stash::stash::{Bond, Stash, Tether};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::join;
use tracing::error;

pub struct MailUserContext {
    this: Weak<Self>,
    mail_context: Arc<MailContext>,
    user_context: Arc<UserContext>,
    cache: Cache,
    exclusive: MailUserContextExclusive,
}

impl MailUserContext {
    /// Create a new user context.
    pub(crate) async fn new(
        mail_context: Arc<MailContext>,
        user_context: Arc<UserContext>,
    ) -> MailContextResult<Arc<Self>> {
        let stash = user_context.stash().clone();
        let cache_path = mail_context.mail_cache_path(user_context.user_id());
        let cache = Cache::new(cache_path, mail_context.mail_cache_size).await?;
        let action_queue = new_action_queue(stash).await?;
        let user_context_weak = Arc::downgrade(&user_context);
        let exclusive = MailUserContextExclusive::new(EventLoop::new(), action_queue);
        let this = Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            mail_context,
            user_context,
            cache,
            exclusive,
        });

        this.exclusive
            .register_execution_context(Weak::clone(&this.this));
        this.exclusive.register_execution_context(user_context_weak);

        this.init_expiration_loop();

        Ok(this)
    }

    /// Sets a background job where every 60 seconds it deletes all of the messages and conversations
    /// that have an expiration date.
    fn init_expiration_loop(&self) {
        let db = self.user_stash().clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            let mut tether = db.connection();
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                if let Err(e) = Conversation::delete_expired(&mut tether).await {
                    error!("Error in background task deleting expired conversations: {e}");
                }

                if let Err(e) = Message::delete_expired(&mut tether).await {
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

    pub async fn execute_all_actions(&self) -> QueuedResult<usize> {
        self.exclusive.execute_all().await
    }

    pub async fn with_queue<'a, F, T>(&'a self, closure: impl FnOnce(&'a Queue) -> F) -> T
    where
        F: Future<Output = T> + 'a,
    {
        self.exclusive.with_queue(closure).await
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

    /// Get the mail context within which this user context resides.
    pub fn mail_context(&self) -> &MailContext {
        &self.mail_context
    }

    /// Get the inner core context which this context wraps.
    pub fn user_context(&self) -> &UserContext {
        &self.user_context
    }

    /// Get the remote (API) ID of the user associated with this context.
    pub fn user_id(&self) -> &UserId {
        self.user_context.user_id()
    }

    /// Get the remote (API) ID of the session associated with this context.
    pub fn session_id(&self) -> &AuthId {
        self.user_context.session_id()
    }

    /// Provides a way to get the core::models::User instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> MailContextResult<User> {
        let stash = self.user_stash();
        let tether = stash.connection();
        let user_id = self.user_id();
        let real_user = User::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| MailContextError::Other(anyhow!("Missing User, this is a bug.")))?;

        Ok(real_user)
    }

    /// Retrieves the account details of the current account.
    ///
    /// Returns the active account's details or an error if active account does not exist.
    ///
    /// # Errors
    /// - Returns `MailContextError::Other` if the active account is missing.
    pub async fn account_details(&self) -> MailContextResult<AccountDetails> {
        let account_details = self.user_context.account_details().await?;
        Ok(account_details)
    }

    /// Returns the unlocked user keys of this user.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `conn`         - The database connection to load the keys from database.
    ///
    /// # Errors
    /// Returns a wrapped [`MailContextError::KeyHandlingError`] if the operation fails.
    ///
    pub async fn unlocked_user_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        conn: &Tether,
    ) -> MailContextResult<UnlockedUserKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_user_keys(pgp_provider, conn, self)
            .await?;
        Ok(keys)
    }

    /// Returns the unlocked address keys of this user for the provided address.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `conn`         - The database connection to load the keys from database.
    /// * `address_id`   - The address identifier to load the keys for.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    ///
    pub async fn unlocked_address_keys<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        conn: &Tether,
        address_id: &AddressId,
    ) -> MailContextResult<UnlockedAddressKeys<Provider>> {
        let keys = self
            .user_context
            .unlocked_address_keys(pgp_provider, conn, self, address_id)
            .await?;
        Ok(keys)
    }

    /// Loads the send preferences of the recipient with the given email address.
    ///
    /// [`SendPreferences`] contains the send preferences for sending an email to the given recipient
    /// including encryption/signing/formatting options and the encryption key.
    /// The send preferences are used to build the request for sending emails via Proton.
    /// [internal confluence docs](https://protonag.atlassian.net/wiki/spaces/MAILFE/pages/53117391/Send+preferences+for+outgoing+email)
    /// This information is collected from the keys returned by the API, contact vCard data,
    /// sender mail settings, and composer preferences.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider`        - The `OpenPGP` crypto provider from [`proton_crypto_inbox::proton_crypto`].
    /// * `tx `                 - The transaction to query from.
    /// * `email`               - The email address of the recipient.
    /// * `settings`            - The [`CryptoMailSettings`] extracted from the mail settings [`super::models::MailSettings::crypto_mail_settings`]
    /// * `composer_preference` - (currently unused) The composer preferences, use [`ComposerPreference::default()`].
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] or [`proton_crypto_inbox::keys::EncryptionPreferencesError`] if the operation fails.
    ///
    pub async fn recipient_send_preferences<Provider>(
        &self,
        pgp_provider: &Provider,
        tx: &Bond<'_>,
        email: &str,
        settings: CryptoMailSettings,
        composer_preference: ComposerPreference,
    ) -> MailContextResult<SendPreferences<Provider::PublicKey>>
    where
        Provider: PGPProviderSync,
    {
        let encryption_time = crypto_clock::server_crypto_clock().unix_time();

        // If the email is from an owned address by the user, use the corresponding keys.
        if let Some(address) = Address::by_email(email, tx).await.inspect_err(|err| {
            error!("send preferences: failed to search address by email: {err}")
        })? {
            let address_rid = address.remote_id.as_ref().ok_or_else(|| {
                MailContextError::App(AppError::AddressHasNoRemoteId(
                    address.local_id.unwrap_or(LocalAddressId::from(0)),
                ))
            })?;

            let address_keys = self
                .unlocked_address_keys(pgp_provider, tx, address_rid)
                .await
                .inspect_err(|err| error!("send preferences for self: {err}"))?;
            let send_preferences =
                SendPreferences::new_for_self(&address_keys, encryption_time, settings)
                    .inspect_err(|err| error!("send preferences for self: {err}"))?;
            return Ok(send_preferences);
        }

        let user_keys = self.unlocked_user_keys(pgp_provider, tx).await?;
        // Fetch API keys, and contact-pinned keys concurrently.
        let (api_keys_result, vcard_keys_result) = join!(
            self.user_context
                .public_address_keys(pgp_provider, email, false),
            self.user_context.public_address_keys_from_contacts(
                pgp_provider,
                tx,
                &user_keys,
                email
            )
        );

        // Handle error when loading contact keys, but ignore CardNotFound as it's valid to have no contact.
        if let Err(e) = &vcard_keys_result {
            if !matches!(
                e,
                CoreContextError::ContactError(ContactError::CardNotFound(_))
            ) {
                error!(
                    "send preferences: failed to load contact pinned keys: {}",
                    e
                );
            }
        }

        // On error, we currently assume no pinned keys exists.
        let vcard_keys = vcard_keys_result.ok().flatten();

        let send_preferences = SendPreferences::new(
            api_keys_result?,
            vcard_keys,
            encryption_time,
            &settings,
            composer_preference,
        )
        .inspect_err(|err| error!("send preferences: {err}"))?;

        Ok(send_preferences)
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
        self.user_context
            .session()
            .api()
            .get_tests_ping(None, None)
            .await?;
        Ok(())
    }

    /// Get the connection status of the current user session.
    pub async fn connection_status(&self) -> ConnectionStatus {
        self.user_context.connection_status().await
    }
}

impl LoadKeySecret for MailUserContext {
    fn key_secret(&self) -> impl Future<Output = Option<UserKeySecret>> {
        self.session().expose_key_secret()
    }
}
