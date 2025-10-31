use crate::actions::MailActionError;
use crate::datatypes::LocalMessageId;
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension as _};
use proton_mail_api::services::proton::ProtonMail as _;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, StashError, Tether};
use std::mem;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteAllMessagesInLabel {
    local_id: LocalLabelId,
    ids_for_rollback: Vec<LocalMessageId>,
}

impl DeleteAllMessagesInLabel {
    pub async fn new(local_id: LocalLabelId, tether: &Tether) -> Result<Self, StashError> {
        let ids = Message::ids_in_label(local_id, tether).await?;
        Ok(Self {
            local_id,
            ids_for_rollback: ids,
        })
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

impl Handler for DeleteAllMessagesInLabelHandler {
    type Action = DeleteAllMessagesInLabel;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_deleted(action.ids_for_rollback.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(mem::take(&mut action.ids_for_rollback), tx).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let id = Label::local_id_counterpart(action.local_id, guard.tether())
            .await?
            .ok_or_else(|| {
                error!("remote_id not found for local_label_id (trying to empty a local folder?)");
                LabelError::CouldNotResolveRemoteLabel(action.local_id)
            })?;

        info!("Deleting all messages in {id}");

        self.api.delete_all_messages_in_label(id).await?;

        Ok(())
    }
    async fn rebase_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        //TODO(ET-5183): Test me!
        self.apply_local(this_id, action, tx).await?;
        Ok(())
    }
}
