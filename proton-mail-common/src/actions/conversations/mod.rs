use crate::actions::ActionError;
use crate::models::{Conversation, Label as LabelModel, ModelError};
use proton_api_mail::services::proton::response_data::OperationResult;
use proton_core_common::datatypes::{LabelId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use tracing::error;

mod delete;
mod label;

mod mark_read;
mod mark_unread;
mod r#move;
mod unlabel;

pub use delete::Delete;
pub use label::Label;
pub use mark_read::MarkRead;
pub use mark_unread::MarkUnread;
pub use r#move::Move;
pub use unlabel::Unlabel;

/// Convenience type which contains data common to many conversation actions.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionData {
    /// Local label id which this action applies to.
    label_id: u64,
    /// Resolved remote label id.
    ///
    /// Note: this is only for user with remote execution, it should be set by then.
    remote_label_id: Option<LabelId>,
    /// Local conversation ids for the action to act on.
    ids: Vec<u64>,
    /// Resolved remote conversation ids.
    remote_ids: Vec<RemoteId>,
}

impl ActionData {
    /// Create a new instance with the given `label_id` and conversation `ids`.
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            ids: Vec::from_iter(ids),
            label_id,
            remote_label_id: None,
            remote_ids: vec![],
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
        if self.ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        self.remote_label_id = Some(find_remote_label_id(tx, self.label_id).await?);

        let conv_ids = Conversation::find_remote_ids(self.ids.clone(), tx)
            .await
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                e
            })?;

        self.remote_ids = conv_ids;

        Ok(())
    }
}

/// Resolve the remote id for a label with `local_id`.
///
/// # Errors
///
/// Returns error if the resolution failed.
async fn find_remote_label_id(tether: &Tether, local_id: u64) -> Result<LabelId, ActionError> {
    let Some(label_id) = LabelModel::find_remote_id(local_id, tether).await? else {
        return Err(ModelError::LabelNotFound(local_id).into());
    };

    Ok(label_id)
}

/// Filter server response for conversations on which the operation failed.
pub fn filter_conversation_responses(responses: Vec<OperationResult>) -> Vec<RemoteId> {
    responses
        .into_iter()
        .filter(|r| r.response.code != 1000)
        .map(|r| RemoteId::from(r.id))
        .collect::<Vec<_>>()
}
