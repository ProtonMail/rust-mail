#![allow(async_fn_in_trait)]
//! This module contains mail specific push notifications.
//!
//! It's using shared base from [`proton_core_common`] but with the context of mail application
//!

use proton_core_common::datatypes::EncryptedPushNotification;
use proton_crypto_account::proton_crypto;
use serde::Deserialize;
use std::sync::Arc;
use tracing::error;

use crate::{MailContext, MailContextError};

/// Decrypted notification usable only in the context of the Inbox application
///
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecryptedInboxPushNotification {
    // TODO (ET-2204): Obviously this is not the final datastructure shape,
    // just a proof of concept
    Email {},
    OpenUrl {},
}

/// Notification specific for the Inbox, that can be decrypted and deserialized
///
pub trait DecryptableInboxPushNotification {
    /// Decrypt and deserialize generic push notification into Inbox-specific notification
    ///
    async fn into_decrypted_inbox_mail_notification(
        self,
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError>;
}

impl DecryptableInboxPushNotification for EncryptedPushNotification {
    async fn into_decrypted_inbox_mail_notification(
        self,
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedInboxPushNotification, MailContextError> {
        let pgp_provider = proton_crypto::new_pgp_provider();

        let auth_id = &self.auth_id;
        let Some(session) = ctx.get_session(auth_id.clone()).await? else {
            error!("Could not find a session with id {auth_id}");
            return Err(MailContextError::SessionMissing(auth_id.clone()));
        };
        let ctx = ctx.user_context_from_session(&session, None).await?;
        let tether = ctx.user_stash().connection();
        let user_keys = ctx.unlocked_user_keys(&pgp_provider, &tether).await?;

        let decrypted_notification = self
            .into_decrypted_push_notification(&pgp_provider, &user_keys)
            .inspect_err(|e| error!("Failed to decrypt mail notification: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        let decrypted_mail_notification = decrypted_notification.notification.inner;

        Ok(decrypted_mail_notification)
    }
}
