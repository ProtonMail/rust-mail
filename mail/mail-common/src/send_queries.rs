use crate::datatypes::LocalMessageId;
use crate::models::DraftMetadata;
use crate::{MailContext, MailContextError};
use proton_core_api::services::proton::UserId;
use proton_core_common::CoreSessionState;

use std::sync::Arc;

impl MailContext {
    /// Get all unsent message ids for given `user_id`.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to retrieve the user context or retrieve the messages.
    pub async fn get_unsent_messages_ids_for_user(
        self: &Arc<Self>,
        user_id: UserId,
    ) -> Result<Vec<LocalMessageId>, MailContextError> {
        let session = self.get_account_sessions(user_id.clone()).await?.pop();

        let msg_ids = match session {
            Some(session)
                if matches!(
                    CoreSessionState::of(&session),
                    CoreSessionState::Authenticated
                ) =>
            {
                let Some(user_ctx) = self.initialized_user_context_from_session(&session).await?
                else {
                    return Err(MailContextError::UserContextNotInitialized(user_id));
                };
                let tether = user_ctx.user_stash().connection().await?;
                DraftMetadata::messages_with_pending_send(&tether).await?
            }
            _ => vec![],
        };

        Ok(msg_ids)
    }
}
