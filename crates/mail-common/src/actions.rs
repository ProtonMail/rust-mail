mod available_action;
pub mod conversations;
pub mod labels;
pub mod messages;

pub use self::available_action::*;
use crate::datatypes::{ExclusiveLocation, RollbackItemType};
use crate::models::{Label, RollbackItem};
use crate::AppError;
use itertools::Itertools;
use proton_action_queue::action::Factory;
use proton_api_core::service::ApiServiceError;
use proton_api_mail::services::proton::response_data::OperationResult;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;
use std::marker::PhantomData;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("App: {0}")]
    App(#[from] AppError),
    #[error("No input provided")]
    NoInput,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for ActionError {
    fn request_error(&self) -> Option<&ApiServiceError> {
        match self {
            Self::Http(e) => Some(e),
            _ => None,
        }
    }
}

pub(crate) fn new_action_factory() -> Factory {
    let mut factory = Factory::new();
    const ERR_MSG: &str = "Double Factory registration";
    factory.register::<conversations::Delete>().expect(ERR_MSG);
    factory.register::<conversations::Unlabel>().expect(ERR_MSG);
    factory.register::<conversations::Label>().expect(ERR_MSG);
    factory
        .register::<conversations::MarkRead>()
        .expect(ERR_MSG);
    factory
        .register::<conversations::MarkUnread>()
        .expect(ERR_MSG);
    factory.register::<conversations::Move>().expect(ERR_MSG);
    factory.register::<messages::label::Label>().expect(ERR_MSG);
    factory
        .register::<messages::unlabel::Unlabel>()
        .expect(ERR_MSG);
    factory
}

/// Convenience type which contains data common to many actions.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct GenericActionData<T>
where
    T: Model,
{
    /// Local label id which this action applies to.
    label_id: LocalId,
    /// Resolved remote label id.
    ///
    /// Note: this is only for user with remote execution, it should be set by then.
    remote_label_id: Option<LabelId>,
    /// Local ids for the action to act on.
    target_ids: Vec<LocalId>,
    /// Resolved remote ids.
    remote_target_ids: Vec<RemoteId>,
    phantom: PhantomData<T>,
}

impl<T> GenericActionData<T>
where
    T: Model,
{
    /// Create a new instance with the given `label_id` and target `ids`.
    pub fn new(label_id: LocalId, target_ids: impl IntoIterator<Item = LocalId>) -> Self {
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
    async fn resolve_ids(&mut self, tx: &Tether) -> Result<(), ActionError> {
        if self.target_ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        self.remote_label_id = Some(Label::resolve_remote_label_id(self.label_id, tx).await?);

        let conv_ids = LocalId::counterparts::<T, _>(self.target_ids.clone(), tx)
            .await
            .map_err(|e| {
                error!("Failed to resolve ids: {e}");
                e
            })?;

        self.remote_target_ids = conv_ids;

        Ok(())
    }

    /// Mark the action items to be rollback
    async fn mark_rollback(
        &self,
        item_type: RollbackItemType,
        tx: &Tether,
    ) -> Result<(), ActionError> {
        for remote_id in self.remote_target_ids.iter() {
            RollbackItem::new(remote_id.clone(), item_type)
                .save_using(tx)
                .await?;
        }

        Ok(())
    }
}

/// Filter server response on which the operation failed.
pub fn filter_responses(responses: Vec<OperationResult>) -> Vec<RemoteId> {
    responses
        .into_iter()
        .filter(|r| r.response.code != 1000)
        .map(|r| RemoteId::from(r.id))
        .collect::<Vec<_>>()
}

/// Action which moves target items between two labels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionMoveData<T>
where
    T: Model,
{
    /// The current label whether the items are locate.
    source_label_id: LocalId,
    /// The destination label where the items should move to.
    destination_label_id: LocalId,
    /// Resolved remote id for the destination label.
    remote_destination_label_id: Option<LabelId>,
    /// Local item ids that need to be moved.
    target_ids: Vec<LocalId>,
    /// Resolved remote conversation ids.
    remote_target_ids: Vec<RemoteId>,
    phantom_data: PhantomData<T>,
}

impl<T> ActionMoveData<T>
where
    T: Model,
{
    /// Create a new action which moves items with `target_ids` from `source_label_id` to
    ///`destination_label_id`.
    pub fn new(
        source_label_id: LocalId,
        destination_label_id: LocalId,
        target_ids: impl IntoIterator<Item = LocalId>,
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
    async fn resolve_ids(&mut self, tx: &Tether) -> Result<(), ActionError> {
        self.remote_destination_label_id =
            Some(Label::resolve_remote_label_id(self.destination_label_id, tx).await?);
        self.remote_target_ids = LocalId::counterparts::<T, _>(self.target_ids.clone(), tx)
            .await
            .inspect_err(|e| error!("Failed to resolve ids: {e}"))?;
        Ok(())
    }
}

/// Action which change all the labels of messages or conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelAsData<T>
where
    T: Model,
{
    source_label_id: LocalId,
    local_ids: Vec<LocalId>,
    remote_ids: Vec<RemoteId>,
    local_selected_label_ids: Vec<LocalId>,
    remote_selected_label_ids: Vec<RemoteId>,
    local_partially_selected_label_ids: Vec<LocalId>,
    remote_partially_selected_label_ids: Vec<RemoteId>,
    original_labels: HashMap<LocalId, Vec<LocalId>>,
    original_locations: HashMap<LocalId, Option<ExclusiveLocation>>,
    must_archive: bool,
    phantom_data: PhantomData<T>,
}

impl<T> LabelAsData<T>
where
    T: Model,
{
    fn new(
        source_label_id: LocalId,
        local_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
        must_archive: bool,
    ) -> Self {
        Self {
            source_label_id,
            local_ids,
            remote_ids: vec![],
            local_selected_label_ids: selected_label_ids,
            remote_selected_label_ids: vec![],
            local_partially_selected_label_ids: partially_selected_label_ids,
            remote_partially_selected_label_ids: vec![],
            original_labels: HashMap::new(),
            original_locations: HashMap::new(),
            must_archive,
            phantom_data: PhantomData,
        }
    }

    /// Resolve all local ids into the remote counterpart.
    async fn resolve_remote_ids(&mut self, tx: &Tether) -> Result<(), ActionError> {
        self.remote_ids = LocalId::counterparts::<T, _>(self.local_ids.clone(), tx).await?;
        let remote_selected_label_ids =
            LocalId::counterparts::<Label, _>(self.local_selected_label_ids.clone(), tx).await?;
        self.remote_selected_label_ids = remote_selected_label_ids.into_iter().map_into().collect();
        let remote_partially_selected_label_ids =
            LocalId::counterparts::<Label, _>(self.local_partially_selected_label_ids.clone(), tx)
                .await?;
        self.remote_partially_selected_label_ids = remote_partially_selected_label_ids
            .into_iter()
            .map_into()
            .collect();
        Ok(())
    }
}
