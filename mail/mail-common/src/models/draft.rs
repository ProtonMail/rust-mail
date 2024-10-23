use crate::draft::ReplyMode;
use proton_core_common::datatypes::{LocalId, RemoteId};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};

/// Represents some metadata associated with a draft that we can't retrieve
/// from existing models that is required to satisfy the remote request.
///
/// This is only required when creating new drafts. As soon as a draft
/// is created on the server, we can delete this data as it is no longer
/// required.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("new_draft_metadata")]
pub struct NewDraftMetadata {
    /// Id of the draft message.
    #[IdField]
    pub local_message_id: LocalId,
    /// Remote id of the message being replied to.
    #[DbField]
    pub remote_parent_id: Option<RemoteId>,
    /// Reply mode used for the draft, if `None` is an empty draft.
    #[DbField]
    pub reply_mode: Option<ReplyMode>,
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl NewDraftMetadata {
    /// Find metadata for a message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn find_by_id<A>(
        local_message_id: LocalId,
        interface: &A,
    ) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        NewDraftMetadata::find_first(
            "WHERE local_message_id=?",
            params![local_message_id],
            interface,
        )
        .await
    }

    /// Delete metadata for a message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn delete<A>(local_message_id: LocalId, interface: &A) -> Result<usize, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        interface
            .execute(
                format!(
                    "DELETE FROM `{}` WHERE local_message_id = ?",
                    Self::table_name()
                ),
                params![local_message_id],
            )
            .await
    }
}
