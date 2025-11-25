use crate::actions::MailActionError;
use crate::datatypes::{LocalMessageId, MessageFlags, MimeType};
use crate::models::Message;
use crate::{AppError, MailUserContext};
use anyhow::Context as _;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_action_queue::rebase::{RebaseChangeSet, RebaseKey};
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::sync::Weak;
use tracing::info;

/// Action which reports a message as phishing.
///
/// It will also blacklist the sender so that next messages also go to spam.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReportPhishing {
    message_id: LocalMessageId,
}

impl ReportPhishing {
    /// Create a new instance which reports a message as phishing
    /// Don't call this directly, use [`Message::action_report_phishing`] instead.
    pub fn new(message_id: LocalMessageId) -> Self {
        Self { message_id }
    }
}

impl Action for ReportPhishing {
    const TYPE: Type = Type("report_phishing");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ReportPhishingHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_optional_ext(self.message_id)
            .build()
    }
}

pub struct ReportPhishingHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for ReportPhishingHandler {
    type Action = ReportPhishing;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::set_flags(action.message_id, MessageFlags::PHISHING_MANUAL, bond).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::unset_flags(action.message_id, MessageFlags::PHISHING_MANUAL, bond).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let ctx = self.ctx.upgrade().ok_or(MailActionError::LostContext)?;
        let tether = guard.tether();

        let body = Message::message_body(&ctx, action.message_id)
            .await
            .context("Failed to get message body")
            .map_err(AppError::Other)?;

        let remote_id = Message::local_id_counterpart(action.message_id, tether)
            .await?
            .ok_or_else(|| AppError::MessageHasNoRemoteId(action.message_id))?;

        let mime_type = MimeType::from(body.mime_type).into();

        info!("Reporting phishing for {remote_id:?}");

        ctx.session()
            .report_phishing(remote_id, mime_type, &body.body)
            .await?;

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        changeset: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let rebase_key: RebaseKey = action.message_id.into();
        if changeset.contains(&rebase_key) {
            Message::set_flags(action.message_id, MessageFlags::PHISHING_MANUAL, tx).await?;
        }
        Ok(())
    }
}
