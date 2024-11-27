use crate::draft::ReplyMode;
use proton_core_common::datatypes::LocalId;
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use proton_sqlite3::rusqlite::ToSql;
use serde::{Deserialize, Serialize};
use stash::exports::SqliteError;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, StashError};
use std::fmt::{Display, Formatter};

/// Identifier for draft [`DraftMetadata`]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct MetadataId(pub u64);

impl Display for MetadataId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromSql for MetadataId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u64::column_result(value).map(MetadataId)
    }
}

impl ToSql for MetadataId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

/// Represents some metadata associated with a draft that we can't retrieve
/// from existing models that is required to satisfy the remote request.
///
/// This metadata will be created for every draft we open or create so it
/// can be kept up to date with ongoing changes.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("draft_metadata")]
pub struct DraftMetadata {
    #[IdField(autoincrement)]
    pub id: Option<MetadataId>,
    /// Id of the draft message.
    #[DbField]
    pub local_message_id: Option<LocalId>,
    #[DbField]
    /// Id of the conversation this draft belongs to.
    pub local_conversation_id: Option<LocalId>,
    /// Local id of the message being replied to.
    #[DbField]
    pub local_parent_id: Option<LocalId>,
    /// Reply mode used for the draft, if `None` is an empty draft.
    #[DbField]
    pub reply_mode: Option<ReplyMode>,
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl DraftMetadata {
    /// Create metadata for new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn empty<A>(interface: &A) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut metadata = Self {
            id: None,
            local_message_id: None,
            local_conversation_id: None,
            local_parent_id: None,
            reply_mode: None,
            row_id: None,
        };

        metadata.save_using(interface).await?;

        Ok(metadata)
    }

    /// Create metadata for new reply draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn reply<A>(
        reply_mode: ReplyMode,
        source_message_id: LocalId,
        source_conversation_id: LocalId,
        interface: &A,
    ) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut metadata = Self {
            id: None,
            local_message_id: None,
            local_conversation_id: Some(source_conversation_id),
            local_parent_id: Some(source_message_id),
            reply_mode: Some(reply_mode),
            row_id: None,
        };

        metadata.save_using(interface).await?;

        Ok(metadata)
    }

    /// Find metadata with `id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn find_by_id<A>(id: MetadataId, interface: &A) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        DraftMetadata::find_first("WHERE id=?", params![id], interface).await
    }

    /// Find metadata for a message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn find_by_message_id<A>(
        local_message_id: LocalId,
        interface: &A,
    ) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        DraftMetadata::find_first(
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
    pub async fn delete_for_message<A>(
        local_message_id: LocalId,
        interface: &A,
    ) -> Result<usize, StashError>
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

    /// Delete metadata for the given `id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn delete<A>(id: MetadataId, interface: &A) -> Result<usize, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        interface
            .execute(
                format!("DELETE FROM `{}` WHERE id = ?", Self::table_name()),
                params![id],
            )
            .await
    }
}
