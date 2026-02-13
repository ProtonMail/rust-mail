use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::datatypes::LocalMessageId;
use crate::models::{ConversationCounter, LabelExt, Message, MessageCounter};
use anyhow::anyhow;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelExtension};
use proton_mail_api::services::proton::ProtonMail as _;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::mem;
use std::sync::Weak;
use tracing::{info, instrument, warn};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteAllMessagesInLabel {
    local_id: LocalLabelId,
    ids_for_rollback: Vec<LocalMessageId>,

    #[serde(default)]
    prev_msg_total: Option<u64>,

    #[serde(default)]
    prev_msg_unread: Option<u64>,

    #[serde(default)]
    prev_conv_total: Option<u64>,

    #[serde(default)]
    prev_conv_unread: Option<u64>,
}

impl DeleteAllMessagesInLabel {
    pub async fn new(local_id: LocalLabelId, tether: &Tether) -> Result<Option<Self>, LabelError> {
        let ids_for_rollback = Message::ids_in_label(local_id, tether).await?;

        let label = Label::find_by_id(local_id, tether)
            .await?
            .ok_or_else(|| LabelError::CouldNotResolveRemoteLabel(local_id))?;

        if label.is_idle(tether).await? {
            Ok(Some(Self {
                local_id,
                ids_for_rollback,
                prev_msg_total: None,
                prev_msg_unread: None,
                prev_conv_total: None,
                prev_conv_unread: None,
            }))
        } else {
            // If a label is already being emptied, scheduling another emptying
            // would fail on the `apply_remote()` stage as the backend doesn't
            // allow for parallel empty-ings to run.
            //
            // At the same time, we don't want for this action to fail, because
            // some people might be tempted to press the "delete all" button
            // multiple times in a row, thinking it might speed the process up
            // or something (let's call it "the elevator button phenomenon").
            //
            // So instead let's just silently ignore the action.
            warn!(
                "Label {local_id} is already busy, no point scheduling another \
                 delete-all action for it",
            );

            Ok(None)
        }
    }
}

impl Action<UserDb> for DeleteAllMessagesInLabel {
    const TYPE: Type = Type("delete_all_messages_in_label");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = DeleteAllMessagesInLabelHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_required_related(self.local_id)
            .with_required_many_ext(self.ids_for_rollback.iter().copied())
            .build()
    }
}

pub struct DeleteAllMessagesInLabelHandler {
    pub api: Session,
    pub ctx: Weak<MailUserContext>,
}

impl DeleteAllMessagesInLabelHandler {
    #[instrument(skip_all)]
    async fn label(
        &self,
        action: &DeleteAllMessagesInLabel,
        tether: &Tether,
    ) -> Result<Label, LabelError> {
        Label::find_by_id(action.local_id, tether)
            .await?
            .ok_or_else(|| LabelError::CouldNotResolveRemoteLabel(action.local_id))
    }

    /// Message and conversation counters are only decremented by the number of
    /// messages and conversations we know of locally - this might be less than
    /// the amount of objects that actually exist in the label.
    ///
    /// Since we know that this action removes all messages and conversations,
    /// we can just set the counters to zero up-front.
    async fn reset_counters(
        &self,
        tx: &Bond<'_>,
        mut action: Option<&mut DeleteAllMessagesInLabel>,
        label: &Label,
    ) -> Result<(), MailActionError> {
        if let Some(mut counters) = MessageCounter::find_by_id(label.id(), tx).await? {
            if let Some(action) = &mut action {
                action.prev_msg_total = Some(counters.total);
                action.prev_msg_unread = Some(counters.unread);
            }

            counters.total = 0;
            counters.unread = 0;
            counters.save(tx).await?;
        }

        if let Some(mut counters) = ConversationCounter::find_by_id(label.id(), tx).await? {
            if let Some(action) = &mut action {
                action.prev_conv_total = Some(counters.total);
                action.prev_conv_unread = Some(counters.unread);
            }

            counters.total = 0;
            counters.unread = 0;
            counters.save(tx).await?;
        }

        Ok(())
    }
}

impl Handler<UserDb> for DeleteAllMessagesInLabelHandler {
    type Action = DeleteAllMessagesInLabel;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let label = self.label(action, tx).await?;

        if label.is_busy(tx).await? {
            // Soft-unreachable, since we validate this in the constructor, but
            // won't hurt to double-check
            return Err(anyhow!("Label {} is busy", label.id()).into());
        }

        label.mark_busy(tx).await?;

        self.reset_counters(tx, Some(action), &label).await?;

        Message::mark_deleted(action.ids_for_rollback.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let label = self.label(action, tx).await?;

        label.mark_idle(tx).await?;
        let message_ids = mem::take(&mut action.ids_for_rollback);
        Message::mark_undeleted(message_ids, tx).await?;

        // Note: No need to re-index on undelete. Message content hasn't changed.

        if let Some(total) = action.prev_msg_total
            && let Some(unread) = action.prev_msg_unread
        {
            MessageCounter {
                local_label_id: label.id(),
                total,
                unread,
            }
            .save(tx)
            .await?;
        }

        if let Some(total) = action.prev_conv_total
            && let Some(unread) = action.prev_conv_unread
        {
            ConversationCounter {
                local_label_id: label.id(),
                total,
                unread,
            }
            .save(tx)
            .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_, UserDb>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let label = self.label(action, guard.tether()).await?;

        info!(
            local_id = ?label.local_id,
            remote_id = ?label.remote_id,
            "Deleting all messages"
        );

        if let Some(remote_id) = &label.remote_id {
            self.api
                .delete_all_messages_in_label(remote_id.clone())
                .await?;

            // Emptying a label is an asynchronous action - even though backend
            // responds immediately, the action is actually carried out in the
            // background.
            //
            // That's why we cannot mark the label as idle back again here.
        }

        // Note: Search removal is handled by the event loop when the server
        // sends delete events. See user_context/events/messages.rs.
        // This ensures ALL deleted messages are removed from the index,
        // including ones not yet synced to this client.

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let label = self.label(action, tx).await?;

        // Since new conversation and messages/conversations can be added to
        // this label while the action is active, we need to always recalculate
        // until we have support for delete up to
        action.ids_for_rollback = Message::ids_in_label_with_deleted(action.local_id, tx).await?;

        Message::mark_deleted(action.ids_for_rollback.clone(), tx).await?;

        // Note that we don't save counters back to `action` here - that's
        // because with all likelihood the changeset here contains just a subset
        // of the messages we've deleted before and there's no way to know the
        // overlap.
        //
        // (unless we stored the deleted ids into `action`, but come on.)
        self.reset_counters(tx, None, &label).await?;

        Ok(())
    }
}
