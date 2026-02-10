use crate::CoreContextError;
use crate::actions::dependency_builder::ActionDependencyKeysBuilder;
use crate::datatypes::LocalContactId;
use crate::models::{Contact, ModelExtension, ModelIdExtension};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::ContactId;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete {
    local_ids: Vec<LocalContactId>,
    remote_ids: Vec<ContactId>,
}

impl Delete {
    #[must_use]
    pub fn new(local_ids: Vec<LocalContactId>) -> Self {
        Self {
            local_ids,
            remote_ids: Vec::new(),
        }
    }
}

impl Action<UserDb> for Delete {
    const TYPE: Type = Type("delete_contacts");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = DeleteHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = CoreContextError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::new()
            .with_optional_many_ext(self.local_ids.iter().copied())
            .build()
    }
}

pub struct DeleteHandler {
    pub api: Session,
}

impl Handler<UserDb> for DeleteHandler {
    type Action = Delete;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let contacts = Contact::find_by_ids(action.local_ids.clone(), tx).await?;

        action.remote_ids = contacts
            .iter()
            .filter_map(|c| c.remote_id.clone())
            .collect();

        for mut contact in contacts {
            contact.mark_delete(tx).await?;
        }

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let contacts = Contact::find_by_ids(action.local_ids.clone(), tx).await?;

        for mut contact in contacts {
            contact.mark_undelete(tx).await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_, UserDb>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let failed = Contact::delete_from_remote(&action.remote_ids, &self.api).await?;
        let mut failed_local_ids = Vec::with_capacity(failed.len());

        if failed.is_empty() {
            Ok(())
        } else {
            let conn = guard.tether();

            for remote_id in failed {
                let Some(local_id) = Contact::remote_id_counterpart(remote_id, conn).await? else {
                    continue;
                };

                failed_local_ids.push(local_id);
            }

            action.local_ids = failed_local_ids;

            Err(CoreContextError::Other(anyhow::anyhow!(
                "Failed to delete contacts from remote"
            )))
        }
    }

    async fn rebase_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        //TODO(ET-5183): Test me!
        self.apply_local(this_id, action, tx).await
    }
}
