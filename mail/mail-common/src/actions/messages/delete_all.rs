use crate::actions::MailActionError;
use crate::datatypes::LocalMessageId;
use crate::models::Message;
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
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::mem;
use tracing::{info, instrument, warn};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteAllMessagesInLabel {
    local_id: LocalLabelId,
    ids_for_rollback: Vec<LocalMessageId>,
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

impl Action for DeleteAllMessagesInLabel {
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
}

impl Handler for DeleteAllMessagesInLabelHandler {
    type Action = DeleteAllMessagesInLabel;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let label = self.label(action, tx).await?;

        if label.is_busy(tx).await? {
            // Soft-unreachable, since we validate this in the constructor, but
            // won't hurt to double-check
            return Err(anyhow!("Label {} is busy", label.id()).into());
        }

        label.mark_busy(tx).await?;
        Message::mark_deleted(action.ids_for_rollback.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let label = self.label(action, tx).await?;

        label.mark_idle(tx).await?;
        Message::mark_undeleted(mem::take(&mut action.ids_for_rollback), tx).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
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

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Since new conversation and messages/conversations can be added to this label
        // while the action is active, we need to always recalculate until we have support
        // for delete up to.
        action.ids_for_rollback = Message::ids_in_label_with_deleted(action.local_id, tx).await?;

        Message::mark_deleted(action.ids_for_rollback.clone(), tx).await?;

        Ok(())
    }
}
