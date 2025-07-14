use crate::datatypes::LocalMessageId;
use crate::rsvp::RsvpMailSender;
use crate::{AppError, MailContextError, MailContextResult};
use crate::{MailUserContext, models::Message};
use anyhow::Context;
use proton_calendar_common::{self as cal, RsvpAnswer, RsvpAnswerError, RsvpAnswerStatus};
use proton_crypto_inbox::proton_crypto;
use stash::orm::Model;
use stash::stash::Tether;
use std::ops;
use tracing::{info, instrument, warn};

#[derive(Clone, Debug)]
pub struct RsvpEvent {
    event: cal::RsvpEvent,
    msg_id: LocalMessageId,
}

impl RsvpEvent {
    pub(crate) fn new(event: cal::RsvpEvent, msg_id: LocalMessageId) -> Self {
        Self { event, msg_id }
    }

    // TODO (NGC-57) implement support for offline-mode
    #[instrument(
        skip_all,
        fields(id = self.event.raw.as_ref().map(|raw| raw.id.as_str())),
    )]
    pub async fn answer(
        &mut self,
        ctx: &MailUserContext,
        tether: &mut Tether,
        status: RsvpAnswerStatus,
    ) -> MailContextResult<()> {
        info!("Answering RSVP");

        let msg = Message::load(self.msg_id, tether)
            .await
            .context("Couldn't load invite's message")
            .map_err(MailContextError::Other)?
            .ok_or_else(|| AppError::MessageMissing(self.msg_id))?;

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &msg.remote_address_id)
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't unlock address keys");
                err
            })?;

        let sender = {
            let msg_id = msg
                .remote_id
                .as_ref()
                .ok_or_else(|| AppError::MessageHasNoRemoteId(self.msg_id))?;

            let msg_recipient = msg
                .to_list
                .value
                .first()
                .context("Invite's message has no recipient")
                .map_err(MailContextError::Other)?;

            RsvpMailSender {
                ctx,
                pgp: &pgp,
                keys: &keys,
                tether,
                msg_id,
                msg_subject: &msg.subject,
                msg_recipient,
                msg_address_id: &msg.remote_address_id,
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
