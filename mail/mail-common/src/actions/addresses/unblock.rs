use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::services::proton::{IncomingDefaultId, PrivateEmail, Proton};
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::params;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unblock {
    pub email: PrivateEmail,
}

impl Action for Unblock {
    const TYPE: Type = Type("unblock");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnblockHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UnblockHandler {
    pub api: Proton,
}

impl Handler for UnblockHandler {
    type Action = Unblock;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tracing::info!("Unblocking {}", action.email);

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
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tracing::info!("Restoring block for {}", action.email);

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
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tracing::info!("Unblocking {}", action.email);

        let id = guard
            .tether()
            .query_value::<_, IncomingDefaultId>(
                "SELECT id AS value FROM incoming_default WHERE email = ?",
                params![action.email.clone()],
            )
            .await?;

        self.api.delete_incoming_default(&id).await?;

        Ok(())
    }
}
