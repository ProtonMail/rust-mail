use crate::MailContextError;
use crate::actions::PREFETCH_ROLLBACK_ACTION_GROUP;
use crate::models::RollbackItem;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionGroup, ActionId, DefaultVersionConverter, Handler,
    Priority, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

/// This action flushes the rollback items.
///
/// Rollback items are items that need to refetched from the server when something fails. The
/// failure often happens because we are out of sync.
#[derive(Debug, Serialize, Deserialize)]
pub struct RollbackAction {}

const ROLLBACK_BATCH_SIZE: usize = 50;

impl Action for RollbackAction {
    const TYPE: Type = Type("item_rollback");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Low;

    const GROUP: ActionGroup = PREFETCH_ROLLBACK_ACTION_GROUP;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = RollbackActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new().build()
    }
}

pub struct RollbackActionHandler {
    pub api: Session,
}

impl Handler for RollbackActionHandler {
    type Action = RollbackAction;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
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
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        RollbackItem::sync_all(&self.api, &mut writer_guard, Some(ROLLBACK_BATCH_SIZE)).await?;

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
