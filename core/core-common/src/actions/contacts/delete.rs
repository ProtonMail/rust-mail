use crate::datatypes::{LocalId, RemoteId};
use crate::models::{Contact, ModelExtension};
use crate::{Context, CoreContextError};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete {
    local_ids: Vec<LocalId>,
    remote_ids: Vec<RemoteId>,
}

impl Delete {
    pub fn new(local_ids: Vec<LocalId>) -> Self {
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

    type Context = Context;
}

#[derive(Default)]
pub struct Handler;

impl proton_action_queue::action::Handler for Handler {
    type Action = Delete;
    type Context = Context;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
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
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let contacts = Contact::find_by_ids(action.local_ids.clone(), tx).await?;

        for mut contact in contacts {
            contact.mark_undelete(tx).await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let failed = Contact::delete_from_remote(&action.remote_ids, session.api()).await?;

        if !failed.is_empty() {
            let tx = stash.transaction().await?;
            for remote_id in failed {
                let Some(mut contact) = Contact::find_by_id(remote_id.clone(), &tx).await? else {
                    tracing::warn!("Failed to find contact with remote id: {:?}", remote_id);
                    continue;
                };

                contact.mark_undelete(stash).await?;
            }
            tx.commit().await?;
        }

        Ok(())
    }
}
