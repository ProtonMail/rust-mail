use crate::actions::MailActionError;
use crate::datatypes::LocalMessageId;
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension as _};
use proton_mail_api::services::proton::ProtonMail as _;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::mem;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteAllMessagesInLabel {
    local_id: LocalLabelId,
    ids_for_rollback: Vec<LocalMessageId>,
}

impl DeleteAllMessagesInLabel {
    pub fn new(local_id: LocalLabelId) -> Self {
        Self {
            local_id,
            ids_for_rollback: vec![],
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
}

pub struct DeleteAllMessagesInLabelHandler {
    pub api: Proton,
}

impl Handler for DeleteAllMessagesInLabelHandler {
    type Action = DeleteAllMessagesInLabel;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let ids = Message::ids_in_label(action.local_id, tx).await?;

        Message::mark_deleted(ids.clone(), tx).await?;
        action.ids_for_rollback = ids;

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
}
