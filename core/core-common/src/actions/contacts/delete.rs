use crate::datatypes::{IdCounterpart, LocalContactId};
use crate::models::{Contact, ModelExtension};
use crate::{CoreContextError, UserContext};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::common::ContactId;
use proton_api_core::session::CoreSession;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Stash};

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

impl Action for Delete {
    const TYPE: Type = Type("delete_contacts");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = CoreContextError;

    type Context = UserContext;
}

#[derive(Default)]
pub struct Handler;

impl proton_action_queue::action::Handler for Handler {
    type Action = Delete;
    type Context = UserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let contacts = Contact::find_by_ids(action.local_ids.clone(), tx).await?;

        for mut contact in contacts {
            contact.mark_undelete(tx).await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let failed = Contact::delete_from_remote(&action.remote_ids, ctx.session().api()).await?;
        let mut failed_local_ids = Vec::with_capacity(failed.len());

        if failed.is_empty() {
            Ok(())
        } else {
            let conn = stash.connection();
            for remote_id in failed {
                let Some(local_id) = remote_id.counterpart::<Contact>(&conn).await? else {
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
}
