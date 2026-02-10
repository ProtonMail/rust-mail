use std::sync::Weak;

use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::IncomingDefault;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncIncomingDefaults;

impl Action<UserDb> for SyncIncomingDefaults {
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

impl Handler<UserDb> for SyncIncomingDefaultsHandler {
    type Action = SyncIncomingDefaults;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        mut guard: WriterGuard<'_, UserDb>,
    ) -> Result<
        <Self::Action as Action<UserDb>>::RemoteOutput,
        <Self::Action as Action<UserDb>>::Error,
    > {
        let data = IncomingDefault::sync(&self.api).await?;

        tracing::info!("Updating incoming defaults");

        guard
            .tx::<_, _, <Self::Action as Action<UserDb>>::Error>(async |tx| {
                IncomingDefault::replace_all(
                    data.into_iter().map(IncomingDefault::from).collect(),
                    tx,
                )
                .await?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }
}
