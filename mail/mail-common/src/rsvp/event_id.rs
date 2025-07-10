use crate::datatypes::LocalMessageId;
use crate::{MailContextResult, MailUserContext, RsvpEvent};
use proton_calendar_common::{self as cal};
use proton_core_api::services::proton::AddressId;
use proton_crypto_inbox::proton_crypto;
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

        let now = ctx.mail_context().core_context().clock().now();

        match self
            .id
            .fetch(ctx.api(), &pgp, &keys, ctx.rsvp_cache(), &now)
            .await
        {
            Ok(event) => {
                if let Some(event) = event {
                    Ok(Some(RsvpEvent::new(event, self.msg_id)))
                } else {
                    // Can happen if user has disabled the invite auto-import
                    // feature
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
