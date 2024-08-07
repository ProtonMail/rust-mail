use crate::actions::conversations::filter_conversation_responses;
use crate::actions::ActionError;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{LabelId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Action which moves conversations between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move {
    /// The current label whether the conversations are locate.
    source_label_id: u64,
    /// The destination label where the conversations should move to.
    destination_label_id: u64,
    /// Resolved remote id for the source label.
    remote_source_label_id: Option<LabelId>,
    /// Resolved remote id for the destination label.
    remote_destination_id: Option<LabelId>,
    /// Local conversation ids that need to be moved.
    ids: Vec<u64>,
    /// Resolved remote conversation ids.
    remote_ids: Vec<RemoteId>,
}

impl Move {
    /// Create a new action which moves conversations with `ids` from `source_label_id` to
    ///`destination_label_id`.
    pub fn new(
        source_label_id: u64,
        destination_label_id: u64,
        ids: impl IntoIterator<Item = u64>,
    ) -> Self {
        Self {
            source_label_id,
            destination_label_id,
            ids: Vec::from_iter(ids),
            remote_ids: vec![],
            remote_source_label_id: None,
            remote_destination_id: None,
        }
    }
}

impl Action for Move {
    const TYPE: Type = Type("move_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Move;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        let (remote_source_id, remote_destination_id) = Conversation::move_conversations(
            action.source_label_id,
            action.destination_label_id,
            action.ids.clone(),
            tx,
        )
        .await?;

        let remote_ids = Conversation::find_remote_ids(action.ids.clone(), tx)
            .await
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                e
            })?;

        action.remote_destination_id = Some(remote_destination_id);
        action.remote_source_label_id = Some(remote_source_id);
        action.remote_ids = remote_ids;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::move_conversations(
            action.destination_label_id,
            action.source_label_id,
            action.ids.clone(),
            tx,
        )
        .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        let responses = Conversation::apply_label_to_multiple_remote::<Proton>(
            action.remote_destination_id.clone().expect("should be set"),
            action.remote_ids.clone(),
            None,
            session.api(),
        )
        .await?;

        let failed_ids = filter_conversation_responses(responses);

        if failed_ids.is_empty() {
            return Ok(());
        }

        error!("Move operation failed for: {:?}", failed_ids);

        let tx = stash.transaction().await?;
        let local_ids = Conversation::find_local_ids(failed_ids.clone(), &tx).await?;

        Conversation::move_conversations(
            action.destination_label_id,
            action.source_label_id,
            local_ids,
            &tx,
        )
        .await?;

        tx.commit().await?;

        Ok(())
    }
}
