pub mod addresses;
mod available_action;
pub mod conversations;
pub mod draft;
pub mod event_poll;
pub mod labels;
pub mod messages;

pub use self::available_action::*;
use crate::AppError;
use crate::datatypes::{ExclusiveLocation, RollbackItemType};
use crate::models::RollbackItem;
use addresses::block;
use indoc::formatdoc;
use itertools::Itertools;
use proton_action_queue::action::{Action, FactoryError, WriterGuardError};
use proton_action_queue::queue::Queue;
use proton_api_core::consts::General;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::{LabelId, ProtonIdMarker};
use proton_api_mail::services::proton::response_data::OperationResult;
use proton_core_common::action_queue::CoreActionError;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, LabelError, ModelIdExtension};
use proton_sqlite3::rusqlite::ToSql;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, StashError, Tether};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum MailActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("App: {0}")]
    App(#[from] AppError),
    #[error("Label: {0}")]
    Label(#[from] LabelError),
    #[error("No input provided")]
    NoInput,
    #[error("Queue Writer Guard Expired")]
    QueueWriterGuardExpired,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for MailActionError {
    fn is_network_failure(&self) -> bool {
        if let Self::Http(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

impl From<WriterGuardError> for MailActionError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => Self::Stash(e),
        }
    }
}

impl From<CoreActionError> for MailActionError {
    fn from(value: CoreActionError) -> Self {
        match value {
            CoreActionError::Http(api_service_error) => Self::Http(api_service_error),
            CoreActionError::Stash(stash_error) => Self::Stash(stash_error),
            CoreActionError::Label(label_error) => Self::Label(label_error),
            CoreActionError::NoInput => Self::NoInput,
            CoreActionError::QueueWriterGuardExpired => Self::QueueWriterGuardExpired,
            CoreActionError::Other(error) => Self::Other(error),
        }
    }
}

pub(crate) fn register_mail_actions(queue: &Queue) {
    fn register_action<T: Action>(queue: &Queue) {
        if let Err(e) = queue.register::<T>() {
            match e {
                FactoryError::AlreadyRegistered(_) => {
                    // Do nothing it is possible we already registered this action
                    // in the queue once before.
                }
                e => {
                    panic!("Failed to register action: {e:?}");
                }
            }
        }
    }

    register_action::<conversations::Delete>(queue);
    register_action::<conversations::Unlabel>(queue);
    register_action::<conversations::Label>(queue);
    register_action::<conversations::MarkRead>(queue);
    register_action::<conversations::MarkUnread>(queue);
    register_action::<block::Block>(queue);
    register_action::<conversations::Move>(queue);
    register_action::<messages::label::Label>(queue);
    register_action::<messages::unlabel::Unlabel>(queue);
    register_action::<messages::r#move::Move>(queue);
    register_action::<messages::delete::Delete>(queue);
    register_action::<messages::read::Read>(queue);
    register_action::<messages::unread::Unread>(queue);
    register_action::<messages::ham::Ham>(queue);
    register_action::<draft::Save>(queue);
    register_action::<draft::Send>(queue);
    register_action::<labels::Expand>(queue);
    register_action::<messages::label_as::LabelAs>(queue);
    register_action::<conversations::label_as::LabelAs>(queue);
    register_action::<proton_core_common::actions::contacts::Delete>(queue);
    register_action::<draft::Discard>(queue);
    register_action::<draft::UndoSend>(queue);
    register_action::<draft::AttachmentUpload>(queue);
    register_action::<draft::AttachmentRemove>(queue);
    register_action::<event_poll::EventPoll>(queue);
}

/// Convenience type which contains data common to many actions.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct GenericActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Local label id which this action applies to.
    label_id: LocalLabelId,
    /// Resolved remote label id.
    ///
    /// Note: this is only for user with remote execution, it should be set by then.
    remote_label_id: Option<LabelId>,
    /// Local ids for the action to act on.
    target_ids: Vec<T::IdType>,
    /// Resolved remote ids.
    remote_target_ids: Vec<T::RemoteId>,
    phantom: PhantomData<T>,
}

impl<T> GenericActionData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Create a new instance with the given `label_id` and target `ids`.
    pub fn new(label_id: LocalLabelId, target_ids: impl IntoIterator<Item = T::IdType>) -> Self {
        Self {
            label_id,
            remote_label_id: None,
            target_ids: Vec::from_iter(target_ids),
            remote_target_ids: vec![],
            phantom: PhantomData,
        }
    }

    /// Resolve all remote ids.
    ///
    /// Resolved remote ids are stored on self.
    ///
    /// # Errors
    ///
    /// Returns error if ids could not be resolved.
    async fn resolve_ids(&mut self, tether: &Tether) -> Result<(), MailActionError> {
        if self.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        self.remote_label_id = Some(Label::resolve_remote_label_id(self.label_id, tether).await?);

        let conv_ids = T::local_ids_counterpart(self.target_ids.clone(), tether)
            .await
            .map_err(|e| {
                error!("Failed to resolve ids: {e:?}");
                e
            })?;

        self.remote_target_ids = conv_ids;

        Ok(())
    }

    /// Return the ids of all the items which do not have a remote id.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    async fn unsynced_item_ids(&self, tether: &Tether) -> Result<Vec<T::IdType>, MailActionError> {
        let placeholders = stash::utils::placeholders(self.target_ids.len());
        #[allow(trivial_casts)]
        let values = self
            .target_ids
            .iter()
            .map(|id| Box::new(id.clone()) as Box<dyn ToSql + Send>)
            .collect();
        Ok(tether
            .query_values::<_, T::IdType>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} IN ({})
                            AND
                                {} IS NULL
                            ",
                    T::id_field_name(),
                    T::table_name(),
                    T::id_field_name(),
                    placeholders,
                    T::remote_id_field_name(),
                ),
                values,
            )
            .await?)
    }

    /// Mark the action items to be rollback
    async fn mark_rollback(
        &self,
        item_type: RollbackItemType,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        for remote_id in self.remote_target_ids.iter() {
            RollbackItem::new(remote_id.to_string(), item_type)
                .save(tx)
                .await?;
        }

        Ok(())
    }
}

/// Filter server response on which the operation failed.
pub fn filter_responses<T: ProtonIdMarker>(responses: Vec<OperationResult<T>>) -> Vec<T> {
    filter_responses_by_codes(responses, &[General::NoError as u32])
}

/// Filter server response on which the operation failed.
pub fn filter_responses_by_codes<T: ProtonIdMarker>(
    responses: Vec<OperationResult<T>>,
    accepted: &[u32],
) -> Vec<T> {
    responses
        .into_iter()
        .filter(|r| !accepted.contains(&r.response.code))
        .map(|r| r.id)
        .collect::<Vec<_>>()
}

/// Action which moves target items between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionMoveData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// The current label whether the items are locate.
    source_label_id: LocalLabelId,
    /// The destination label where the items should move to.
    destination_label_id: LocalLabelId,
    /// Resolved remote id for the destination label.
    remote_destination_label_id: Option<LabelId>,
    /// Local item ids that need to be moved.
    target_ids: Vec<T::IdType>,
    /// Resolved remote conversation ids.
    remote_target_ids: Vec<T::RemoteId>,
    phantom_data: PhantomData<T>,
}

impl<T> ActionMoveData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned>,
{
    /// Create a new action which moves items with `target_ids` from `source_label_id` to
    ///`destination_label_id`.
    pub fn new(
        source_label_id: LocalLabelId,
        destination_label_id: LocalLabelId,
        target_ids: impl IntoIterator<Item = T::IdType>,
    ) -> Self {
        Self {
            source_label_id,
            destination_label_id,
            remote_destination_label_id: None,
            target_ids: Vec::from_iter(target_ids),
            remote_target_ids: vec![],
            phantom_data: PhantomData,
        }
    }

    /// Resolve all remote ids
    ///
    /// # Errors
    ///
    /// * if some id can not be resolved
    async fn resolve_ids(&mut self, tx: &Bond<'_>) -> Result<(), MailActionError> {
        self.remote_destination_label_id =
            Some(Label::resolve_remote_label_id(self.destination_label_id, tx).await?);
        self.remote_target_ids = T::local_ids_counterpart(self.target_ids.clone(), tx)
            .await
            .inspect_err(|e| error!("Failed to resolve ids: {e:?}"))?;
        Ok(())
    }
}

/// Action which change all the labels of messages or conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAsData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned + Eq + PartialEq + Hash>,
{
    source_label_id: LocalLabelId,
    local_ids: Vec<T::IdType>,
    remote_ids: Vec<T::RemoteId>,
    local_all_label_ids: Vec<LocalLabelId>,
    remote_all_label_ids: Vec<LabelId>,
    local_selected_label_ids: Vec<LocalLabelId>,
    remote_selected_label_ids: Vec<LabelId>,
    local_partially_selected_label_ids: Vec<LocalLabelId>,
    remote_partially_selected_label_ids: Vec<LabelId>,
    must_archive: bool,
    added_labels: HashMap<T::IdType, HashSet<LocalLabelId>>,
    removed_labels: HashMap<T::IdType, HashSet<LocalLabelId>>,
    original_location: HashMap<T::IdType, Option<ExclusiveLocation>>,
    phantom_data: PhantomData<T>,
}

impl<T> LabelAsData<T>
where
    T: ModelIdExtension<IdType: Serialize + DeserializeOwned + Eq + PartialEq + Hash>,
{
    fn new(
        source_label_id: LocalLabelId,
        local_ids: Vec<T::IdType>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Self {
        Self {
            source_label_id,
            local_ids,
            local_all_label_ids: vec![],
            remote_all_label_ids: vec![],
            remote_ids: vec![],
            local_selected_label_ids: selected_label_ids,
            remote_selected_label_ids: vec![],
            local_partially_selected_label_ids: partially_selected_label_ids,
            remote_partially_selected_label_ids: vec![],
            must_archive,
            added_labels: HashMap::new(),
            removed_labels: HashMap::new(),
            original_location: HashMap::new(),
            phantom_data: PhantomData,
        }
    }

    /// Resolve all local ids into the remote counterpart.
    async fn resolve_remote_ids(&mut self, tx: &Bond<'_>) -> Result<(), MailActionError> {
        self.remote_ids = T::local_ids_counterpart(self.local_ids.clone(), tx).await?;
        self.remote_all_label_ids =
            Label::local_ids_counterpart(self.local_all_label_ids.clone(), tx).await?;
        let remote_selected_label_ids =
            Label::local_ids_counterpart(self.local_selected_label_ids.clone(), tx).await?;
        self.remote_selected_label_ids = remote_selected_label_ids.into_iter().map_into().collect();
        let remote_partially_selected_label_ids =
            Label::local_ids_counterpart(self.local_partially_selected_label_ids.clone(), tx)
                .await?;
        self.remote_partially_selected_label_ids = remote_partially_selected_label_ids
            .into_iter()
            .map_into()
            .collect();
        Ok(())
    }

    async fn mark_rollback(
        &self,
        kind: RollbackItemType,
        bond: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        for remote_id in &self.remote_ids {
            RollbackItem::new(remote_id.to_string(), kind)
                .save(bond)
                .await?;
        }
        Ok(())
    }
}
