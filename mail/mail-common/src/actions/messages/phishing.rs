use crate::actions::MailActionError;
use crate::datatypes::{LocalMessageId, MessageFlags, SystemLabelId};
use crate::models::Message;
use crate::{AppError, MailUserContext};
use anyhow::Context as _;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_api_core::services::proton::LabelId;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

/// Action which reports a message as phishing.
///
/// It will also blacklist the sender so that next messages also go to spam.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReportPhishing {
    label_id: LocalLabelId,
    message_id: LocalMessageId,
}

impl ReportPhishing {
    /// Create a new instance which reports a message as phishing
    pub fn new(label_id: LocalLabelId, message_id: LocalMessageId) -> Self {
        Self {
            label_id,
            message_id,
        }
    }
}

impl Action for ReportPhishing {
    const TYPE: Type = Type("report_phishing");
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

impl ActionHandler for Handler {
    type Action = ReportPhishing;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let spam = Label::remote_id_counterpart(LabelId::spam(), bond)
            .await?
            .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(LabelId::spam()))?;

        Message::move_messages(action.label_id, spam, vec![action.message_id], bond).await?;
        Message::set_flags(action.message_id, MessageFlags::PHISHING_MANUAL, bond).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let spam = Label::remote_id_counterpart(LabelId::spam(), bond)
            .await?
            .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(LabelId::spam()))?;

        Message::move_messages(spam, action.label_id, vec![action.message_id], bond).await?;
        Message::unset_flags(action.message_id, MessageFlags::PHISHING_MANUAL, bond).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let tether = guard.tether();

        let body = Message::message_body(ctx, action.message_id)
            .await
            .context("Failed to get message body")
            .map_err(AppError::Other)?;

        let remote_id = Message::local_id_counterpart(action.message_id, tether)
            .await?
            .ok_or_else(|| AppError::MessageHasNoRemoteId(action.message_id))?;

        ctx.api()
            .report_phishing(remote_id, body.metadata.mime_type.into(), &body.body)
            .await?;

        Ok(())
    }
}
