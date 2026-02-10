use crate::datatypes::LocalMessageId;
use crate::models::{Message, MessageBodyMetadata};
use crate::rsvp::RsvpKeys;
use crate::{AppError, MailContextError, MailContextResult, MailUserContext, RsvpEvent};
use anyhow::Context;
use proton_calendar_common::{self as cal, RsvpFetchError};
use proton_core_common::models::Address;
use proton_crypto_inbox::proton_crypto;
use stash::orm::Model;
use stash::stash::Tether;
use std::ops;
use tracing::{debug, info, instrument, warn};

#[derive(Clone, Debug)]
pub struct RsvpEventId {
    id: cal::RsvpEventId,
    msg_id: LocalMessageId,
    msg_meta: MessageBodyMetadata,
}

impl RsvpEventId {
    pub(crate) fn new(
        id: cal::RsvpEventId,
        msg_id: LocalMessageId,
        msg_meta: MessageBodyMetadata,
    ) -> Self {
        Self {
            id,
            msg_id,
            msg_meta,
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
        let keys = RsvpKeys::new(ctx, tether);
        let rsvp_service = ctx.rsvp_service();
        let cache = rsvp_service.cache();
        let contacts = rsvp_service.contacts();

        let now = ctx.mail_context().core_context().clock().now();

        let msg = Message::load(self.msg_id, tether)
            .await
            .context("Couldn't load invite's message")
            .map_err(MailContextError::Other)?
            .ok_or_else(|| AppError::MessageMissing(self.msg_id))?;

        let addr = Address::load(msg.local_address_id, tether)
            .await
            .context("Couldn't load invite's message's address")?
            .ok_or_else(|| AppError::AddressMissing(msg.local_address_id))?;

        let user = ctx.user().await?;
        let week_start = ctx.user_settings().await?.week_start.into();

        match self
            .id
            .fetch(
                ctx.session(),
                &pgp,
                &keys,
                cache,
                contacts,
                &now,
                &addr.email,
                week_start,
            )
            .await
        {
            Ok(event) => {
                if let Some(event) = event {
                    Ok(Some(RsvpEvent::new(
                        event,
                        msg,
                        self.msg_meta.clone(),
                        addr,
                        user,
                    )))
                } else {
                    debug!("False-positive, not a valid invite");

                    Ok(None)
                }
            }

            Err(err) => {
                warn!(?err, "Couldn't fetch event from the calendar");

                Err(match err {
                    RsvpFetchError::Keys(err) => err,
                    RsvpFetchError::Rsvp(err) => err.into(),
                })
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
