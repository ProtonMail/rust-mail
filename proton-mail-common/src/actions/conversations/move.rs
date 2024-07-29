use crate::actions::conversations::{filter_conversation_responses, resolve_remote_label_id};
use crate::actions::ActionError;
use crate::datatypes::SystemLabelId;
use crate::models::{Conversation, ConversationLabel, Label};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{LabelId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Tether;
use tracing::error;

/// Action which moves conversations between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move {
    /// Whether the source label is a movable folder.
    is_movable_folder: bool,
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
    #[serde(skip)]
    /// Remote conversations id which failed to apply.
    failed_ids: Vec<RemoteId>,
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
            failed_ids: vec![],
            remote_source_label_id: None,
            remote_destination_id: None,
            is_movable_folder: false,
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

        let Some(source_label) = Label::load_using(action.source_label_id, tx).await? else {
            return Err(ActionError::LabelNotFound(action.source_label_id));
        };

        action.is_movable_folder = source_label.is_movable_folder();

        let Some(remote_active_label_id) = source_label.remote_id else {
            return Err(ActionError::LabelHasNoRemoteId(action.source_label_id));
        };

        let remote_destination_label_id =
            resolve_remote_label_id(tx, action.destination_label_id).await?;

        let remote_ids = Conversation::find_remote_ids(action.ids.clone(), tx)
            .await
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                e
            })?;

        // If moving to trash, mark conversations as read.
        if remote_destination_label_id == LabelId::trash() {
            Conversation::mark_multiple_as_read(action.ids.clone(), tx)
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations as read when moving to trash: {e}");
                    e
                })?
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if remote_destination_label_id == LabelId::trash()
            || remote_destination_label_id == LabelId::spam()
        {
            let all_mail_id = Label::find_local_ids(vec![LabelId::all_mail()], tx).await?;
            if all_mail_id.is_empty() {
                return Err(ActionError::RemoteLabelNotFound(LabelId::all_mail()));
            }

            let all_mail_local_id = all_mail_id[0];

            for &local_conversation_id in &action.ids {
                let label_ids =
                    ConversationLabel::labels_ids_for_conversation(local_conversation_id, tx)
                        .await?;
                for label_id in label_ids.into_iter().filter(|id| *id != all_mail_local_id) {
                    Conversation::remove_label_from_multiple(
                        label_id,
                        vec![local_conversation_id],
                        tx,
                    )
                        .await.map_err(|e| {
                        error!("Failed to remove label {label_id} from conv {local_conversation_id} when moving into spam/trash:{e}");
                        e
                    })?;
                }
            }
            // When moving out of Trash or Spam, add AlmostAllMail label
        } else if remote_active_label_id == LabelId::trash()
            || remote_active_label_id == LabelId::spam()
        {
            let almost_all_mail_id =
                Label::find_local_ids(vec![LabelId::almost_all_mail()], tx).await?;
            if almost_all_mail_id.is_empty() {
                return Err(ActionError::RemoteLabelNotFound(LabelId::almost_all_mail()));
            }

            let almost_all_mail_local_id = almost_all_mail_id[0];
            Conversation::apply_label_to_multiple(almost_all_mail_local_id, action.ids.clone(), tx)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to apply almost all mail label when moving out of spam/trash:{e}"
                    );
                    e
                })?;
        }

        if action.is_movable_folder {
            Conversation::remove_label_from_multiple(action.source_label_id, action.ids.clone(), tx)
                .await?
        }

        Conversation::apply_label_to_multiple(action.destination_label_id, action.ids.clone(), tx)
            .await?;

        action.remote_destination_id = Some(remote_destination_label_id);
        action.remote_source_label_id = Some(remote_active_label_id);
        action.remote_ids = remote_ids;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.is_movable_folder {
            Conversation::apply_label_to_multiple(action.source_label_id, action.ids.clone(), tx)
                .await?;
        }
        Conversation::remove_label_from_multiple(
            action.destination_label_id,
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
    ) -> Result<(), <Self::Action as Action>::Error> {
        let responses = Conversation::apply_label_to_multiple_remote::<Proton>(
            action.remote_destination_id.clone().expect("should be set"),
            action.remote_ids.clone(),
            None,
            session.api(),
        )
        .await?;

        action.failed_ids = filter_conversation_responses(responses);
        Ok(())
    }

    async fn apply_local_post_remote(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        if action.failed_ids.is_empty() {
            return Ok(());
        }

        error!("Move operation failed for: {:?}", action.failed_ids);
        let local_ids = Conversation::find_local_ids(action.failed_ids.clone(), tx).await?;
        if action.is_movable_folder {
            Conversation::apply_label_to_multiple(action.source_label_id, local_ids.clone(), tx)
                .await?;
        }
        Conversation::remove_label_from_multiple(action.destination_label_id, local_ids, tx)
            .await?;
        Ok(())
    }
}
