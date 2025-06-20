use crate::datatypes::MessageRecipient;
use crate::rsvp::RsvpMailSender;
use crate::{MailContextError, MailContextResult};
use crate::{MailUserContext, models::Message};
use anyhow::Context;
use proton_calendar_common::{self as cal, RsvpAnswer, RsvpAnswerError, RsvpAnswerStatus};
use proton_core_api::services::proton::AddressId;
use proton_crypto_inbox::proton_crypto;
use proton_mail_api::services::proton::common::MessageId;
use stash::stash::Tether;
use std::ops;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct RsvpEvent {
    event: cal::RsvpEvent,
    msg_id: Option<MessageId>,
    msg_subject: String,
    msg_recipient: Option<MessageRecipient>,
    msg_address_id: AddressId,
}

impl RsvpEvent {
    pub(crate) fn new(event: cal::RsvpEvent, msg: &Message) -> Self {
        Self {
            event,
            msg_id: msg.remote_id.clone(),
            msg_subject: msg.subject.clone(),
            msg_recipient: msg.to_list.value.first().cloned(),
            msg_address_id: msg.remote_address_id.clone(),
        }
    }

    /// TODO (NGC-57) implement support for offline-mode
    #[tracing::instrument(
        skip(self, ctx, tether),
        fields(id = self.event.raw.id.as_str()),
    )]
    pub async fn answer(
        &mut self,
        ctx: &MailUserContext,
        tether: &mut Tether,
        status: RsvpAnswerStatus,
    ) -> MailContextResult<()> {
        info!("Answering RSVP");

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &self.msg_address_id)
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't unlock address keys");
                err
            })?;

        let sender = {
            let msg_id = self
                .msg_id
                .as_ref()
                .context("Invite message has no remote id")
                .map_err(MailContextError::Other)?;

            let msg_recipient = self
                .msg_recipient
                .as_ref()
                .context("Invite message has no recipient")
                .map_err(MailContextError::Other)?;

            RsvpMailSender {
                ctx,
                pgp: &pgp,
                keys: &keys,
                tether,
                msg_id,
                msg_subject: &self.msg_subject,
                msg_recipient,
                msg_address_id: &self.msg_address_id,
            }
        };

        let answer = RsvpAnswer {
            now: ctx.mail_context().core_context().clock().now(),
            email: &sender.msg_recipient.address,
            status,
        };

        self.event
            .answer(ctx.api(), &pgp, &keys, ctx.rsvp_cache(), sender, answer)
            .await
            .map_err(|err| match err {
                RsvpAnswerError::Rsvp(err) => err.into(),
                RsvpAnswerError::Mail(err) => err,
            })
    }
}

impl ops::Deref for RsvpEvent {
    type Target = cal::RsvpEvent;

    fn deref(&self) -> &Self::Target {
        &self.event
    }
}
