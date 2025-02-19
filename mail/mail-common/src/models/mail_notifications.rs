#![allow(async_fn_in_trait)]
//! This module contains mail specific push notifications.
//!
//! It's using shared base from [`proton_core_common`] but with the context of mail application
//!

use proton_core_common::datatypes::{EncryptedPushNotification, NotificationKind};
use proton_crypto_account::proton_crypto;
use std::sync::Arc;
use tracing::error;

use crate::{MailContext, MailContextError};

#[derive(Clone, Debug)]
pub enum DecryptedMailPushNotification {
    // TODO (ET-2204): Obviously this is not the final datastructure shape,
    // just a proof of concept
    Email,
    OpenUrl,
}

pub trait DecryptableMailPushNotification {
    async fn into_decrypted_push_mail_notification(
        self,
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedMailPushNotification, MailContextError>;
}

impl DecryptableMailPushNotification for EncryptedPushNotification {
    async fn into_decrypted_push_mail_notification(
        self,
        ctx: Arc<MailContext>,
    ) -> Result<DecryptedMailPushNotification, MailContextError> {
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
            .into_decrypted_push_notification(&user_keys, &pgp_provider)
            .inspect_err(|e| error!("Failed to decrypt mail notification: {e:?}"))
            .map_err(|_| MailContextError::Crypto)?;

        Ok(match decrypted_notification.notification.kind {
            NotificationKind::Email => DecryptedMailPushNotification::Email,
            NotificationKind::OpenUrl => DecryptedMailPushNotification::OpenUrl,
        })
    }
}
