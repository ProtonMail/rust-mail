use crate::models::DraftMetadata;
use crate::{MailContext, MailContextError};
use proton_api_core::services::proton::UserId;
use proton_core_common::CoreSessionState;
use proton_mail_ids::LocalMessageId;

impl MailContext {
    /// Check if any message for all logged in accounts is still pending to send
    ///
    /// # Errors
    ///
    /// Returns error if we failed to retrieve the user context or perform the checks.
    pub async fn has_users_with_unsent_messages(&self) -> Result<bool, MailContextError> {
        let all_user_ctxs = self
            .get_all_logged_in_and_initialized_user_contexts()
            .await?;
        let mut all_messages_were_sent = true;

        for user_ctx in &all_user_ctxs {
            let send_task_count_eq_zero = user_ctx
                .action_queue()
                .typed_actions_count::<crate::actions::draft::Send>()
                .await?
                == 0;

            all_messages_were_sent &= send_task_count_eq_zero;
        }

        Ok(all_messages_were_sent)
    }

    /// Get all unsent message ids for given `user_id`.
    ///
    /// # Errors
    ///
    /// Returns error if we failed to retrieve the user context or retrieve the messages.
    pub async fn get_unsent_messages_ids_for_user(
        &self,
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
                let Some(user_ctx) = self
                    .initialized_user_context_from_session(&session, None)
                    .await?
                else {
                    return Err(MailContextError::UserContextNotInitialized(user_id));
                };
                let tether = user_ctx.user_stash().connection();
                DraftMetadata::messages_with_pending_send(&tether).await?
            }
            _ => vec![],
        };

        Ok(msg_ids)
    }
}
