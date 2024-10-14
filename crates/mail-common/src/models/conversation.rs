use crate::actions::conversations::LabelAs;
use crate::actions::conversations::{Label as ActionLabel, MarkRead, MarkUnread, Move, Unlabel};
use crate::actions::filter_responses;
use crate::datatypes::{LabelType, SystemLabelId};
use crate::models::Label;
use crate::{actions::conversations::Delete, models::Conversation, AppError};
use anyhow::anyhow;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionOutput, Queue};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use stash::stash::{AgnosticInterface, Interface};
use tracing::warn;

impl Conversation {
    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the conversations to label.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_apply_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let action = ActionLabel::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Star multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to star.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_star(
        session: &Session,
        queue: &Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await
            .map_err(|e| ActionError::Queue(e.into()))?
            .expect("Star system label not found");
        let action = ActionLabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unstar multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to unstar.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unstar(
        session: &Session,
        queue: &Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await?
            .expect("Star system label not found");
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the conversations to unlabel.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_remove_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_read(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkRead>, ActionError<MarkRead>> {
        let action = MarkRead::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_unread(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkUnread>, ActionError<MarkUnread>> {
        let action = MarkUnread::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_delete(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Move multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`        - The session.
    /// * `queue`          - The action queue.
    /// * `source_id`      - The ID of the label where the conversations are.
    /// * `destination_id` - The ID of the label where the conversations go.
    /// * `target_ids`     - The IDs of the conversations to move.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_move(
        session: &Session,
        queue: &Queue,
        source_id: LocalId,
        destination_id: LocalId,
        target_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Move>, ActionError<Move>> {
        let action = Move::new(source_id, destination_id, target_ids);
        queue.apply_action(session, action).await
    }

    /// Soft delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the current view.
    /// * `conversation_ids` - The IDs of the converstations to delete.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_deleted(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: impl IntoIterator<Item = LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Action to change labels on a batch of conversations.
    ///
    /// All given conversations will get the selected labels.
    /// All given conversations will keep the partially selected labels.
    /// All given conversations will lose any other labels.
    ///
    /// # Parameters
    ///
    /// * `session`                      - The session.
    /// * `queue`                        - The action queue.
    /// * `source_label_id`              - Id of the currently used label.
    /// * `conversation_ids`             - List of ids of the conversations to label.
    /// * `selected_label_ids`           - List of ids of the Labels to set.
    /// * `partially_selected_label_ids` - List of ids of the Labels to keep as is.
    /// * `must_archive`                 - If true, the given conversations must be archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the action can not be applied.
    ///
    pub async fn action_label_as(
        session: &Session,
        queue: &Queue,
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
        must_archive: bool,
    ) -> Result<bool, AppError> {
        let action = LabelAs::new(
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        );
        match queue
            .apply_action(session, action)
            .await
            .map_err(|e| AppError::Other(anyhow!(e)))?
        {
            ActionStatus::Executed(result) => Ok(result),
            ActionStatus::Queued(id) => Err(AppError::ActionStillQueued(id)),
        }
    }

    /// Locally apply LabelAs action for conversations
    pub(crate) async fn label_as<A>(
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: &[LocalId],
        partially_selected_label_ids: &[LocalId],
        must_archive: bool,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let labels = Label::find_by_kind(LabelType::Label, interface).await?;
        for label in labels {
            let label_id = label.local_id.expect("Should be set");
            if selected_label_ids.contains(&label_id) {
                Self::apply_label(label_id, conversation_ids.clone(), interface).await?
            } else if !partially_selected_label_ids.contains(&label_id) {
                Self::remove_label(label_id, conversation_ids.clone(), interface).await?
            }
            // else keep label as is
        }

        if must_archive {
            let archive_id =
                RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), interface)
                    .await?
                    .expect("Archive label must have a RemoteId");
            Self::move_conversations(source_label_id, archive_id, conversation_ids, interface)
                .await?;
        }

        Ok(())
    }

    /// Remotely apply LabelAs action for conversations
    pub(crate) async fn remote_relabel<A>(
        session: &Session,
        conversation_ids: Vec<RemoteId>,
        selected_label_ids: &[RemoteId],
        partially_selected_label_ids: &[RemoteId],
        interface: &A,
    ) -> Result<Vec<RemoteId>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let api = session.api();

        let conversation_ids: Vec<_> = conversation_ids.into_iter().map_into().collect();
        let labels = Label::find_by_kind(LabelType::Label, interface).await?;
        let mut failed_ids = vec![];
        for label in labels {
            let Some(label_id) = label.remote_id else {
                warn!("Label without remote_id: {label:?}");
                continue;
            };
            if selected_label_ids.contains(&label_id) {
                let response = api
                    .put_conversations_label(
                        conversation_ids.clone(),
                        label_id.into_inner().into(),
                        None,
                    )
                    .await?
                    .responses;
                failed_ids.append(&mut filter_responses(response));
            } else if !partially_selected_label_ids.contains(&label_id) {
                let response = api
                    .put_conversations_unlabel(
                        conversation_ids.clone(),
                        label_id.into_inner().into(),
                    )
                    .await?
                    .responses;
                failed_ids.append(&mut filter_responses(response));
            }
            // else keep label as is
        }
        Ok(failed_ids)
    }
}
