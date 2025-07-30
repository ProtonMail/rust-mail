use crate::draft::compose::validate_sender_address;
use crate::rsvp::RsvpMailSender;
use crate::{AppError, MailContextResult};
use crate::{MailUserContext, models::Message};
use proton_calendar_common::{self as cal, RsvpAnswer, RsvpAnswerError};
use proton_core_common::models::{Address, User};
use proton_crypto_inbox::proton_crypto;
use stash::orm::Model;
use stash::stash::Tether;
use std::ops;
use tracing::{info, instrument, warn};

#[derive(Clone, Debug)]
pub struct RsvpEvent {
    event: cal::RsvpEvent,
    msg: Message,
    addr: Address,
    user: User,
}

impl RsvpEvent {
    pub(crate) fn new(event: cal::RsvpEvent, msg: Message, addr: Address, user: User) -> Self {
        Self {
            event,
            msg,
            addr,
            user,
        }
    }

    #[must_use]
    pub fn is_address_correct(&self) -> bool {
        validate_sender_address(&self.addr, &self.user).is_none()
    }

    #[must_use]
    pub fn can_be_answered(&self) -> bool {
        self.event.can_be_answered() && self.is_address_correct()
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
        answer: RsvpAnswer,
    ) -> MailContextResult<()> {
        info!("Answering RSVP");

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &self.msg.remote_address_id)
            .await
            .inspect_err(|err| warn!(?err, "Couldn't unlock address keys"))?;

        let sender = {
            let msg_id = self
                .msg
                .remote_id
                .as_ref()
                .ok_or_else(|| AppError::MessageHasNoRemoteId(self.msg.id()))?;

            RsvpMailSender {
                ctx,
                pgp: &pgp,
                keys: &keys,
                tether,
                msg_id,
                msg_subject: &self.msg.subject,
                addr_display_name: &self.addr.display_name,
            }
        };

        let now = ctx.mail_context().core_context().clock().now();

        self.event
            .answer(
                ctx.api(),
                &pgp,
                &keys,
                ctx.rsvp_cache(),
                sender,
                &now,
                answer,
            )
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
