use crate::models::RollbackItem;
use crate::{MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
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

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = RollbackActionHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct RollbackActionHandler {}

impl Handler for RollbackActionHandler {
    type Action = RollbackAction;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        // Nothing to do;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        context: &Self::Context,
        _: &mut Self::Action,
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        RollbackItem::sync_all(
            context.session(),
            &mut writer_guard,
            Some(ROLLBACK_BATCH_SIZE),
        )
        .await?;
        Ok(())
    }
}
