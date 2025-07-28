use crate::datatypes::LocalMessageId;
use crate::models::Message;
use crate::{AppError, MailContextError, MailContextResult, MailUserContext, RsvpEvent};
use anyhow::Context;
use proton_calendar_common::{self as cal};
use proton_core_api::services::proton::AddressId;
use proton_crypto_inbox::proton_crypto;
use stash::orm::Model;
use stash::stash::Tether;
use std::ops;
use tracing::{debug, info, instrument, warn};

#[derive(Clone, Debug)]
pub struct RsvpEventId {
    id: cal::RsvpEventId,
    msg_id: LocalMessageId,
    address_id: AddressId,
}

impl RsvpEventId {
    pub(crate) fn new(id: cal::RsvpEventId, msg_id: LocalMessageId, address_id: AddressId) -> Self {
        Self {
            id,
            msg_id,
            address_id,
        }
    }

    // TODO (NGC-57) implement support for offline-mode
    #[instrument(skip_all, fields(id = debug(&self.id)))]
    pub async fn fetch(
        &self,
        ctx: &MailUserContext,
        tether: &mut Tether,
    ) -> MailContextResult<Option<RsvpEvent>> {
        info!("Fetching RSVP");

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &self.address_id)
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't unlock address keys");
                err
            })?;

        let cache = ctx.rsvp_cache();
        let contacts = ctx.rsvp_contacts();
        let now = ctx.mail_context().core_context().clock().now();

        let email = {
            let tether = ctx.user_stash().connection();

            let msg = Message::load(self.msg_id, &tether)
                .await
                .context("Couldn't load invite's message")
                .map_err(MailContextError::Other)?
                .ok_or_else(|| AppError::MessageMissing(self.msg_id))?;

            msg.to_list
                .value
                .first()
                .context("Invite's message has no recipient")
                .map_err(MailContextError::Other)?
                .to_owned()
                .address
        };

        let week_start = ctx.user_settings().await?.week_start.into();

        match self
            .id
            .fetch(
                ctx.api(),
                &pgp,
                &keys,
                cache,
                contacts,
                &now,
                email.as_clear_text_str(),
                week_start,
            )
            .await
        {
            Ok(event) => {
                if let Some(event) = event {
                    Ok(Some(RsvpEvent::new(event, self.msg_id)))
                } else {
                    // Can happen if the `invite.ics` attachment isn't really a
                    // valid invite
                    debug!("False-positive, API says no such event exists");

                    Ok(None)
                }
            }

            Err(err) => {
                warn!(?err, "Couldn't fetch event from the calendar");

                Err(err.into())
            }
        }
    }
}

impl ops::Deref for RsvpEventId {
    type Target = cal::RsvpEventId;

    fn deref(&self) -> &Self::Target {
        &self.id
    }
}
