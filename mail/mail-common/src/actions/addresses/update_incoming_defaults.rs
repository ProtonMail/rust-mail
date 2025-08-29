use crate::actions::MailActionError;
use crate::models::default_location::IncomingDefaultLocation;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncIncomingDefaults;

impl Action for SyncIncomingDefaults {
    const TYPE: Type = Type("update_incoming_defaults");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SyncIncomingDefaultsHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new().build()
    }
}

pub struct SyncIncomingDefaultsHandler {
    pub api: Session,
}

impl Handler for SyncIncomingDefaultsHandler {
    type Action = SyncIncomingDefaults;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let data = IncomingDefaultLocation::sync(&self.api).await?;

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
