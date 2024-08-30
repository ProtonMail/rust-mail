use crate::actions::ActionError;
use crate::datatypes::RollbackItemType;
use crate::models::{Label, Message, RollbackItem};
use crate::AppError;
use proton_api_mail::services::proton::response_data::OperationResult;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use tracing::error;

pub mod label;

/// Convenience type which contains data common to many message actions.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionData {
    /// Local label id which this action applies to.
    label_id: LocalId,
    /// Resolved remote label id.
    ///
    /// Note: this is only for user with remote execution, it should be set by then.
    remote_label_id: Option<LabelId>,
    /// Local message ids for the action to act on.
    message_ids: Vec<LocalId>,
    /// Resolved remote message ids.
    remote_message_ids: Vec<RemoteId>,
}

impl ActionData {
    /// Create a new instance with the given `label_id` and message `ids`.
    pub fn new(label_id: LocalId, message_ids: impl IntoIterator<Item = LocalId>) -> Self {
        Self {
            message_ids: Vec::from_iter(message_ids),
            label_id,
            remote_label_id: None,
            remote_message_ids: vec![],
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
        if self.message_ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        self.remote_label_id = Some(find_remote_label_id(tx, self.label_id).await?);

        let conv_ids = LocalId::counterparts::<Message, _>(self.message_ids.clone(), tx)
            .await
            .map_err(|e| {
                error!("Failed to resolve message ids: {e}");
                e
            })?;

        self.remote_message_ids = conv_ids;

        Ok(())
    }

    /// Mark the action items to be rollback
    async fn mark_rollback_messages(&self, tx: &Tether) -> Result<(), ActionError> {
        for remote_id in self.remote_message_ids.iter() {
            RollbackItem::new(remote_id.clone(), RollbackItemType::Message)
                .save_using(tx)
                .await?;
        }

        Ok(())
    }
}

/// Resolve the remote id for a label with `local_id`.
///
/// # Errors
///
/// Returns error if the resolution failed.
async fn find_remote_label_id(tether: &Tether, local_id: LocalId) -> Result<LabelId, ActionError> {
    let Some(label_id) = local_id.counterpart::<Label, _>(tether).await? else {
        return Err(AppError::LabelNotFound(local_id).into());
    };

    Ok(label_id.into())
}

/// Filter server response for messages on which the operation failed.
pub fn filter_message_responses(responses: Vec<OperationResult>) -> Vec<RemoteId> {
    responses
        .into_iter()
        .filter(|r| r.response.code != 1000)
        .map(|r| RemoteId::from(r.id))
        .collect::<Vec<_>>()
}
