use std::sync::Weak;

use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::IncomingDefault;
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
    pub ctx: Weak<MailUserContext>,
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
        let ctx = self.ctx.upgrade().ok_or(MailActionError::LostContext)?;

        let data = IncomingDefault::sync(&self.api, ctx.core_context().task_service()).await?;

        tracing::info!("Updating incoming defaults");

        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                IncomingDefault::replace_all(
                    data.into_iter()
                        .filter_map(IncomingDefault::from_api)
                        .collect(),
                    tx,
                )
                .await?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}
