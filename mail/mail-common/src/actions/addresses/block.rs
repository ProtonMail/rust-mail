use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_core_api::services::proton::{PrivateEmail, Proton};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use serde::{Deserialize, Serialize};
use stash::params;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
    pub email: PrivateEmail,
}

impl Action for Block {
    const TYPE: Type = Type("block");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = BlockHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct BlockHandler {
    pub api: Proton,
}

impl ActionHandler for BlockHandler {
    type Action = Block;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tracing::info!("Blocking {}", action.email);

        bond.execute(
            "INSERT INTO incoming_default (email, location) VALUES (?,?)",
            params![action.email.clone(), IncomingDefaultLocation::Blocked],
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
        tracing::info!("Removing block for {}", action.email);

        IncomingDefaultLocation::delete_by_email(
            action.email.clone().into_clear_text_string(),
            bond,
        )
        .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tracing::info!("Blocking {}", action.email);

        let new_incoming = self
            .api
            .post_incoming_default(ApiIncomingDefaultLocation::Blocked, &action.email)
            .await?
            .incoming_default;

        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                IncomingDefaultLocation::store_by_email([new_incoming], tx).await?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}
