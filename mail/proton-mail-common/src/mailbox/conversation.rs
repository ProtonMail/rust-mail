use crate::actions::{
    DeleteConversationsAction, LabelConversationsAction, MarkConversationsReadAction,
    MarkConversationsUnreadAction, MoveConversationsAction, UnlabelConversationsAction,
};
use crate::db::{
    ConversationQuery, DBResult, LocalConversation, LocalConversationId, LocalLabelId,
};
use crate::exports::anyhow::anyhow;
use crate::{
    MailContextError, Mailbox, MailboxError, MailboxObservableQueryBuilder, MailboxResult,
};
use bytes::Bytes;
use proton_api_mail::domain::{AddressDomainLogoDetailsBuilder, LabelId, LightOrDarkMode};
use proton_api_mail::proton_api_core::exports::tracing;

impl Mailbox {
    pub async fn sync(&self, conversation_count: usize) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(self.label_id)? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        if let Some(remote_id) = label.rid.clone() {
            tracing::debug!("Syncing {}({})", self.label_id, remote_id);
            let ctx = self.user_ctx.clone();

            let mut initialized = false;
            let connection = ctx.new_db_connection();
            match connection {
                Ok(mut connection) => {
                    let result = connection
                        .tx(|tx| -> DBResult<bool> { tx.check_if_label_is_initialized(label.id) });
                    match result {
                        Ok(value) => initialized = value,
                        Err(e) => tracing::error!("Failed to check if label is initialized: {e}"),
                    }
                }
                Err(e) => tracing::error!("Failed to get db connection: {e}"),
            }
            if initialized {
                tracing::debug!("Label {} already initialized, skipping", self.label_id);
                return Ok(());
            }
            tracing::debug!("Label {} not initialized, fetching", self.label_id);

            let result = ctx
                .sync_first_conversation_page(remote_id, conversation_count)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to sync conversations for labels: {e}");
                    e.into()
                });

            let connection = ctx.new_db_connection();
            match connection {
                Ok(mut connection) => {
                    let result = connection.tx(|tx| -> DBResult<()> {
                        tx.mark_label_as_initialized(label.id)?;
                        Ok(())
                    });
                    if let Err(e) = result {
                        tracing::error!("Failed to mark label as initialized: {e}");
                    }
                }
                Err(e) => tracing::error!("Failed to get db connection: {e}"),
            }
            result
        } else {
            tracing::warn!("Local label {} has no remote id", self.label_id);
            Ok(())
        }
    }

    pub fn new_conversation_query<Builder: MailboxObservableQueryBuilder<ConversationQuery>>(
        &self,
        builder: Builder,
        limit: usize,
    ) -> Builder::Output {
        builder.build(
            self.user_ctx.tracker_service().clone(),
            ConversationQuery::new(self.label_id, limit),
        )
    }

    pub fn conversations(&self, count: usize) -> MailboxResult<Vec<LocalConversation>> {
        let v = self
            .user_ctx
            .conversations_with_context_for_label(self.label_id, count)?;
        Ok(v)
    }

    pub fn delete_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(DeleteConversationsAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn mark_conversations_read(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MarkConversationsReadAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn mark_conversations_unread(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MarkConversationsUnreadAction::new(self.label_id, ids))?;
        Ok(())
    }

    pub fn label_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(LabelConversationsAction::new(label_id, ids))?;
        Ok(())
    }

    pub fn unlabel_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(UnlabelConversationsAction::new(label_id, ids))?;
        Ok(())
    }

    /// Star a conversation. This is the equivalent of adding a conversation to the Starred system
    /// label.
    ///
    /// # Error
    /// Return error if the operation failed.
    pub fn star_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let label_id = self.starred_label_id()?;
        self.label_conversations(label_id, ids)
    }

    /// Unstar a conversation. This is the equivalent of removing a conversation from the Starred
    /// system label.
    ///
    /// # Error
    /// Return error if the operation failed.
    pub fn unstar_conversations(
        &self,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let label_id = self.starred_label_id()?;
        self.unlabel_conversations(label_id, ids)
    }

    /// Move conversations to a given folder.
    pub fn move_conversations(
        &self,
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        self.user_ctx
            .queue_action(MoveConversationsAction::new(self.label_id, label_id, ids))?;
        Ok(())
    }

    /// Move conversations to a given folder with a `remote_id`.
    ///
    /// # Errors
    /// Return error if the action failed, the `remote_id` does not exist or the label
    /// is not a valid destination.
    pub fn move_conversations_with_remote_id(
        &self,
        remote_id: &LabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> MailboxResult<()> {
        let Some(label) = self
            .user_ctx
            .db_read(|conn| conn.label_with_remote_id(remote_id))
            .map_err(MailContextError::from)?
        else {
            return Err(MailboxError::RemoteLabelNotFound(remote_id.clone()));
        };
        if !label.is_movable_folder() {
            return Err(MailboxError::InvalidAction(anyhow!(
                "Destination is not a valid folder"
            )));
        }
        self.user_ctx
            .queue_action(MoveConversationsAction::new(self.label_id, label.id, ids))?;
        Ok(())
    }

    /// Get a logo for a conversation identified by the provided ``conversation_id`` value.  The API request is only made in
    /// the case where neither the mail settings nor the particular sender are configured to prevent a sender image being shown.
    ///
    /// If a logo is to be sought via the API, the logo will be for the first sender in the list included in the conversation.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an issue with the sender that causes
    /// problems when creating the API request on our side.
    pub async fn get_image_for_conversation(
        &self,
        conversation_id: LocalConversationId,
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
    ) -> MailboxResult<Bytes> {
        // this may need updating after completion of ET-181
        if self.user_ctx.mail_settings()?.hide_sender_images {
            // sender images are to be hidden, return nothing
            return Ok(Bytes::new());
        }

        let conversation = self
            .user_ctx
            .db_read(|conn| conn.get_conversation(conversation_id))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::ConversationNotFound(conversation_id))?;

        let sender_for_image = conversation.senders.first().expect("boo"); //TODO ROB - fix this!

        if !sender_for_image.display_sender_image {
            return Ok(Bytes::new());
        }

        let mut address_request_details =
            AddressDomainLogoDetailsBuilder::new().address(sender_for_image.address.clone());

        if let Some(s) = size {
            address_request_details = address_request_details.size(s);
        }

        if let Some(m) = mode {
            address_request_details = address_request_details.mode(m);
        }

        if let Some(bimi_sel) = &sender_for_image.bimi_selector {
            address_request_details = address_request_details.bimi_selector(bimi_sel.clone());
        }

        let address_request_details = address_request_details
            .build()
            .map_err(MailboxError::AddressDomainLogoError)?;

        let session = self.user_ctx.mail_session();
        match session
            .get_address_domain_logo(address_request_details)
            .await
        {
            Ok(response) => Ok(response),
            Err(e) => Err(MailboxError::APIError(e)),
        }
    }

    fn starred_label_id(&self) -> MailboxResult<LocalLabelId> {
        self.user_ctx
            .db_read(|conn| conn.resolve_remote_label_id(LabelId::starred()))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::RemoteLabelNotFound(
                LabelId::starred().clone(),
            ))
    }
}
