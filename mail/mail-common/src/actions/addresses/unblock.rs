use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_core_api::services::proton::IncomingDefaultId;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::params;
use stash::stash::Bond;

/// Action which blocks or unblocks an address
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unblock {
    pub email: String,
}

impl Action for Unblock {
    const TYPE: Type = Type("unblock");
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
    type Action = Unblock;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        bond.execute(
            "UPDATE incoming_default SET location = NULL WHERE email = ?",
            params![action.email.clone()],
        )
        .await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        bond.execute(
            "UPDATE incoming_default SET location = ? WHERE email = ?",
            params![IncomingDefaultLocation::Blocked, action.email.clone()],
        )
        .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let id = guard
            .tether()
            .query_value::<_, IncomingDefaultId>(
                "SELECT id AS value FROM incoming_default WHERE email = ?",
                params![action.email.clone()],
            )
            .await?;

        ctx.api().delete_incoming_default(&id).await?;

        Ok(())
    }
}
