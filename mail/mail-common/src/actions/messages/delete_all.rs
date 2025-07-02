use std::mem;

use crate::models::Message;
use crate::{MailUserContext, actions::MailActionError};
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Type, WriterGuard};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension as _};
use proton_mail_api::services::proton::ProtonMail as _;
use proton_mail_ids::LocalMessageId;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
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
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl proton_action_queue::action::Handler for Handler {
    type Action = DeleteAllMessagesInLabel;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(mem::take(&mut action.ids_for_rollback), tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
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
        ctx.api().delete_all_messages_in_label(id).await?;
        Ok(())
    }
}
