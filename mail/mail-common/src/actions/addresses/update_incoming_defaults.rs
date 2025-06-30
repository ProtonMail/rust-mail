use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

/// Action which blocks or unblocks an address
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncIncomingDefaults;

impl Action for SyncIncomingDefaults {
    const TYPE: Type = Type("update_incoming_defaults");
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
    type Action = SyncIncomingDefaults;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        _: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let data = IncomingDefaultLocation::sync(ctx.api()).await?;
        tracing::info!("Updating incoming defaults");
        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                tx.execute("DELETE FROM incoming_default", vec![]).await?;
                IncomingDefaultLocation::store_by_email(data, tx).await?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}
