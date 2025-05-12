#[cfg(test)]
#[path = "../tests/models/draft_metadata.rs"]
mod draft_metadata;

use crate::MailContextError;
use crate::datatypes::LocalMessageId;
use crate::datatypes::attachment::ContentId;
use crate::draft::{AttachmentError, Error, PackageError, ReplyMode, SaveOrSendError};
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::errors::{DraftSaveSendErrorReason, MailErrorReason, ProtonMailError};
use crate::models::{Attachment, Message, MessageBodyMetadata};
use chrono::Utc;
use derive_more::derive::TryFrom;
use indoc::formatdoc;
use proton_action_queue::action::ActionId;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_ids::{LocalAttachmentId, LocalConversationId};
use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::SqliteError;
use stash::exports::*;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use stash::{params, sql_using_serde};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use tracing::error;
use typed_builder::TypedBuilder;

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
#[derive(Clone, Debug, Eq, Model, PartialEq, TypedBuilder)]
#[TableName("draft_metadata")]
pub struct DraftMetadata {
    #[builder(default, setter(strip_option))]
    #[IdField(autoincrement)]
    pub id: Option<MetadataId>,
    /// Id of the draft message.
    #[builder(default, setter(strip_option))]
    #[DbField]
    pub local_message_id: Option<LocalMessageId>,
    #[builder(default, setter(strip_option))]
    #[DbField]
    /// Id of the conversation this draft belongs to.
    pub local_conversation_id: Option<LocalConversationId>,
    /// Local id of the message being replied to.
    #[builder(default, setter(strip_option))]
    #[DbField]
    pub local_parent_id: Option<LocalMessageId>,
    /// Reply mode used for the draft, if `None` is an empty draft.
    #[builder(default, setter(strip_option))]
    #[DbField]
    pub reply_mode: Option<ReplyMode>,
    /// Last save action id.
    #[builder(default, setter(strip_option))]
    #[DbField]
    pub save_action_id: Option<ActionId>,
    /// Last send action id.
    #[builder(default, setter(strip_option))]
    #[DbField]
    pub send_action_id: Option<ActionId>,

    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[builder(default, setter(strip_option))]
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl DraftMetadata {
    /// Create metadata for new empty draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn empty(bond: &Bond<'_>) -> Result<Self, StashError> {
        let mut metadata = Self {
            id: None,
            local_message_id: None,
            local_conversation_id: None,
            local_parent_id: None,
            reply_mode: None,
            save_action_id: None,
            send_action_id: None,
            row_id: None,
        };

        metadata.save(bond).await?;

        Ok(metadata)
    }

    /// Create metadata for new reply draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn reply(
        reply_mode: ReplyMode,
        source_message_id: LocalMessageId,
        source_conversation_id: LocalConversationId,
        bond: &Bond<'_>,
    ) -> Result<Self, StashError> {
        let mut metadata = Self {
            id: None,
            local_message_id: None,
            local_conversation_id: Some(source_conversation_id),
            local_parent_id: Some(source_message_id),
            reply_mode: Some(reply_mode),
            send_action_id: None,
            save_action_id: None,
            row_id: None,
        };

        metadata.save(bond).await?;

        Ok(metadata)
    }

    /// Find metadata with `id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn find_by_id(id: MetadataId, tether: &Tether) -> Result<Option<Self>, StashError> {
        DraftMetadata::find_first("WHERE id=?", params![id], tether).await
    }

    /// Find metadata for a message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn find_by_message_id(
        local_message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        DraftMetadata::find_first(
            "WHERE local_message_id=?",
            params![local_message_id],
            tether,
        )
        .await
    }

    /// Delete metadata for a message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn delete_for_message(
        local_message_id: LocalMessageId,
        bond: &Bond<'_>,
    ) -> Result<usize, StashError> {
        bond.execute(
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
    pub async fn delete(id: MetadataId, bond: &Bond<'_>) -> Result<usize, StashError> {
        bond.execute(
            format!("DELETE FROM `{}` WHERE id = ?", Self::table_name()),
            params![id],
        )
        .await
    }

    /// Get the message id associated with a draft.
    ///
    /// This method can return `None` if the message has not been
    /// created yet.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn message_id(
        id: MetadataId,
        tether: &Tether,
    ) -> Result<Option<LocalMessageId>, StashError> {
        let Some(metadata) = DraftMetadata::find_by_id(id, tether).await? else {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        Ok(metadata.local_message_id)
    }

    /// Check whether a given message with remote id has an active draft metadata record.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn exists_for_message_with_remote_id(
        remote_id: MessageId,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        let Some(local_id) = Message::remote_id_counterpart(remote_id, tether).await? else {
            return Ok(false);
        };
        Ok(Self::find_by_message_id(local_id, tether).await?.is_some())
    }

    /// Check whether this draft has pending changes that have not been communicated to the server.
    ///
    /// Pending change are action that have been queued but not yet executed.
    ///
    /// # Errors
    ///
    /// Returns errors if the query failed.
    pub async fn has_pending_changes(&self, tether: &Tether) -> Result<bool, StashError> {
        //TODO: check attachment metadata.
        Ok(self.save_action_id.is_some()
            || self.send_action_id.is_some()
            || !DraftAttachmentMetadata::find_attachment_upload_action_ids(
                self.id.unwrap(),
                tether,
            )
            .await?
            .is_empty())
    }

    /// Retrieve the last recorded save action.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn last_save_action_id(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Option<ActionId>, StashError> {
        match tether
            .query_value::<_, Option<ActionId>>(
                format!(
                    "SELECT save_action_id AS value FROM {} WHERE id =?",
                    Self::table_name()
                ),
                params![metadata_id],
            )
            .await
        {
            Ok(action_id) => Ok(action_id),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Retrive all message ids for with send action is pending.
    ///
    /// # Errors
    ///
    /// When database query fails
    ///
    pub async fn messages_with_pending_send(
        tether: &Tether,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let msg_ids = Self::find(
            "WHERE local_message_id IS NOT NULL AND send_action_id IS NOT NULL",
            vec![],
            tether,
        )
        .await?
        .into_iter()
        .filter_map(|draft| draft.local_message_id)
        .collect();

        Ok(msg_ids)
    }
}

/// Due to architectural differences on some of the platforms we need to store the
/// result of the send action in the database rather than relying on the queue observers.
#[derive(Clone, Debug, Eq, Model, PartialEq, Hash)]
#[TableName("draft_send_result")]
pub struct DraftSendResult {
    /// Id of the draft message.
    #[IdField]
    pub local_message_id: LocalMessageId,
    #[DbField]
    /// Only set when the message was sent successfully.
    pub remote_message_id: Option<MessageId>,
    /// Timestamp at which this entry was produced.
    #[DbField]
    pub timestamp: i64,
    /// Timestamp by which we can cancel the sending of this message.
    ///
    #[DbField]
    pub undo_timestamp: i64,
    /// Whether an error occurred while sending the message.
    #[DbField]
    pub error: Option<DraftSendFailure>,
    /// Whether this result was seen at least once.
    #[DbField]
    pub seen: bool,
    #[DbField]
    /// Where this error originated from
    pub origin: DraftSendResultOrigin,
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl DraftSendResult {
    /// Create a new draft send success result for message with `local_message_id` and
    /// the server returned `undo_token`.
    pub fn success(
        local_message_id: LocalMessageId,
        remote_message_id: MessageId,
        undo_timestamp: i64,
    ) -> Self {
        Self {
            local_message_id,
            remote_message_id: Some(remote_message_id),
            timestamp: Utc::now().timestamp(),
            undo_timestamp,
            error: None,
            seen: false,
            origin: DraftSendResultOrigin::Send,
            row_id: None,
        }
    }

    /// Create a new draft send fail result for message with `local_message_id` and
    /// the given `error`.
    pub fn failure(
        local_message_id: LocalMessageId,
        origin: DraftSendResultOrigin,
        error: DraftSendFailure,
    ) -> Self {
        Self {
            local_message_id,
            remote_message_id: None,
            timestamp: Utc::now().timestamp(),
            undo_timestamp: 0,
            seen: false,
            error: Some(error),
            row_id: None,
            origin,
        }
    }

    /// Overwrite `Model::Save` for create or update.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) = Self::find_by_id(self.local_message_id, bond).await? {
            self.row_id = existing.row_id;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Returns all unseen send results.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unseen(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find("WHERE seen=0 ORDER BY timestamp DESC", vec![], tether).await
    }

    /// Returns all unseen send results message ids.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unseen_ids(tether: &Tether) -> Result<Vec<LocalMessageId>, StashError> {
        tether
            .query_values::<_, LocalMessageId>(
                format!(
                    "SELECT local_message_id AS value FROM `{}` WHERE seen=0",
                    Self::table_name()
                ),
                vec![],
            )
            .await
    }

    /// Whether the operation was successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Subscribe to changes made to this database table.
    ///
    /// # Errors
    ///
    /// Returns error if the subscription failed.
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(DraftSendResultTableObserver { sender }))
    }

    /// Set the send results for the messages with `ids` as seen.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn mark_seen(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let params = ids
            .into_iter()
            .map(|id| -> Box<dyn ToSql + Send> { Box::new(id) })
            .collect::<Vec<_>>();

        if params.is_empty() {
            return Ok(());
        }

        bond.execute(
            format!(
                "UPDATE {} SET seen=1 WHERE local_message_id IN ({})",
                Self::table_name(),
                vec!["?"; params.len()].join(",")
            ),
            params,
        )
        .await?;
        Ok(())
    }

    /// Delete the send results for the messages with `ids`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn delete(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let params = ids
            .into_iter()
            .map(|id| -> Box<dyn ToSql + Send> { Box::new(id) })
            .collect::<Vec<_>>();

        if params.is_empty() {
            return Ok(());
        }

        bond.execute(
            format!(
                "DELETE FROM {} WHERE local_message_id IN ({})",
                Self::table_name(),
                vec!["?"; params.len()].join(",")
            ),
            params,
        )
        .await?;
        Ok(())
    }

    /// Returns true whether the current send can be undone as of now.
    #[must_use]
    pub fn is_send_undoable(&self) -> bool {
        let now = Utc::now().timestamp();
        now < self.undo_timestamp
    }

    /// Returns the time left until this message's sending can be cancelled.
    #[must_use]
    pub fn time_left_for_undo(&self) -> Duration {
        let now = Utc::now().timestamp();
        Duration::from_secs(self.undo_timestamp.saturating_sub(now).unsigned_abs())
    }
}

/// Represents the reason why a draft failed to send.
///
/// Unfortunately we can not re-use [`DraftSaveSendErrorReason`] as we can not take ownership of
/// the error so we have to do our own conversion.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailure {
    NoRecipients,
    AddressDoesNotHavePrimaryKey(AddressId),
    RecipientEmailInvalid(String),
    ProtonRecipientDoesNotExist(String),
    UnknownRecipientValidationError(String),
    AddressDisabled(String),
    MessageAlreadySent,
    PackageError(String),
    MessageUpdateIsNotDraft,
    MessageDoesNotExist,
    NoConnection,
    AlreadySent,
    AttachmentUpload(String),
    Server(String),
    Internal,
}

sql_using_serde!(DraftSendFailure);

/// Track the origin/context of this draft status
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum DraftSendResultOrigin {
    /// We failed to update a draft body without sending the message
    Save = 0,
    /// We failed to update a draft body before sending the message
    SaveBeforeSend = 1,
    /// We failed while sending the message
    Send = 2,
    /// We failed while uploading an attachment
    AttachmentUpload = 3,
}

impl ToSql for DraftSendResultOrigin {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::from(*self as u8)))
    }
}

impl FromSql for DraftSendResultOrigin {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl DraftSendFailure {
    /// Returns true if this an error which can be skipped and should not be presented/handled.
    pub fn is_skippable(&self) -> bool {
        // No connection is handled by external queue code
        // Already sent is an error that is expected to occur
        matches!(self, Self::NoConnection | Self::MessageAlreadySent)
    }

    /// Convert from a draft [`Error`]
    #[must_use]
    pub fn from_draft_error(error: &Error) -> Self {
        match error {
            Error::SaveOrSend(err) => match err {
                SaveOrSendError::AddressWithoutPrimaryKey(remote_id) => {
                    Self::AddressDoesNotHavePrimaryKey(remote_id.clone())
                }
                SaveOrSendError::SendMessage(package_error) => {
                    Self::from_draft_package_error(package_error)
                }
                SaveOrSendError::NoRecipients => Self::NoRecipients,
                SaveOrSendError::AlreadySent => Self::AlreadySent,
                _ => Self::Internal,
            },
            Error::Attachment(e) => Self::AttachmentUpload(e.to_string()),
            _ => Self::Internal,
        }
    }

    /// Convert from a draft [`PackageError`]
    #[must_use]
    pub fn from_draft_package_error(value: &PackageError) -> Self {
        match value {
            PackageError::RecipientEmailInvalid(e) => Self::RecipientEmailInvalid(e.clone()),
            PackageError::ProtonRecipientDoesNotExist(e) => {
                Self::ProtonRecipientDoesNotExist(e.clone())
            }
            PackageError::UnknownRecipientValidationError(e) => {
                Self::UnknownRecipientValidationError(e.clone())
            }
            v => Self::PackageError(v.to_string()),
        }
    }

    /// Convert from an [`ApiServiceError`]
    #[must_use]
    pub fn from_api_service_error(error: &ApiServiceError) -> Self {
        if error.is_network_failure() {
            return Self::NoConnection;
        }

        Self::Server(error.to_string())
    }

    /// Convert from a [`MailContextError`]
    #[must_use]
    pub fn from_mail_context_error(value: &MailContextError) -> Self {
        match value {
            MailContextError::Api(error) => Self::from_api_service_error(error),
            MailContextError::Draft(error) => Self::from_draft_error(error),
            _ => Self::Internal,
        }
    }
}

impl From<DraftSendFailure> for ProtonMailError {
    fn from(value: DraftSendFailure) -> Self {
        match value {
            DraftSendFailure::NoRecipients => Self::Reason(MailErrorReason::DraftSaveSendReason(
                DraftSaveSendErrorReason::NoRecipients,
            )),
            DraftSendFailure::AddressDoesNotHavePrimaryKey(v) => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::AddressDoesNotHavePrimaryKey(v),
                ))
            }
            DraftSendFailure::RecipientEmailInvalid(v) => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::RecipientEmailInvalid(v),
                ))
            }
            DraftSendFailure::ProtonRecipientDoesNotExist(v) => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::ProtonRecipientDoesNotExist(v),
                ))
            }
            DraftSendFailure::UnknownRecipientValidationError(v) => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::UnknownRecipientValidationError(v),
                ))
            }
            DraftSendFailure::AddressDisabled(v) => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::AddressDisabled(v)),
            ),
            DraftSendFailure::MessageAlreadySent => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::MessageAlreadySent),
            ),
            DraftSendFailure::PackageError(v) => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::PackageError(v)),
            ),
            DraftSendFailure::MessageUpdateIsNotDraft => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::MessageIsNotADraft),
            ),
            DraftSendFailure::MessageDoesNotExist => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::MessageDoesNotExist),
            ),
            DraftSendFailure::NoConnection => Self::Network,
            DraftSendFailure::Server(v) => {
                // While there is no good conversion to be performed here, it should be very rare
                // that any error we are interested in handling should slip past here.
                // In those cases the error is still logged completely in the action execution
                // code.
                Self::ServerError(UserApiServiceError::OtherHttpError(0, v))
            }
            DraftSendFailure::Internal => Self::Unexpected(Unexpected::Internal),
            DraftSendFailure::AlreadySent => Self::Reason(MailErrorReason::DraftSaveSendReason(
                DraftSaveSendErrorReason::AlreadySent,
            )),
            DraftSendFailure::AttachmentUpload(_) => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::AttachmentUpload),
            ),
        }
    }
}

struct DraftSendResultTableObserver {
    sender: flume::Sender<()>,
}

impl TableObserver for DraftSendResultTableObserver {
    fn tables(&self) -> Vec<String> {
        vec![DraftSendResult::table_name().to_owned()]
    }

    fn on_tables_changed(&self, _: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for DraftSendResultTableObserver: {}",
                    e
                )
            })
            .ok();
    }
}

/// This table tracks the metadata of new attachments added to the draft as well as their
/// state.
#[derive(Clone, Debug, Eq, Model, PartialEq, Hash)]
#[TableName("draft_attachment_metadata")]
pub struct DraftAttachmentMetadata {
    /// Id of the attachment.
    #[IdField]
    pub local_attachment_id: LocalAttachmentId,
    #[DbField]
    /// Draft metadata id.
    pub metadata_id: MetadataId,
    /// Timestamp at which this entry was produced.
    #[DbField]
    timestamp: i64,
    /// Whether an error occurred while uploading the attachment.
    #[DbField]
    state: DraftAttachmentUploadState,
    /// Ownership of this attachment.
    #[DbField]
    pub ownership: DraftAttachmentOwnership,
    /// Last upload error, if any.
    ///
    /// We record the error separate as we need to ascertain the upload state. It's easier
    /// to match against an integer rather than a json object on the database.
    #[DbField]
    pub error: Option<DraftAttachmentUploadError>,
    /// Order in which this attachment should be displayed.
    #[DbField]
    pub display_order: usize,
    /// Upload action that is currently queued or running.
    #[DbField]
    pub action_id: Option<ActionId>,
    /// Whether this entry was removed from the attachment list.
    ///
    /// We keep it hidden until the action succeeds so we can recover.
    #[DbField]
    pub deleted: bool,
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl DraftAttachmentMetadata {
    /// Create a new instance
    pub fn new(
        metadata_id: MetadataId,
        local_attachment_id: LocalAttachmentId,
        display_order: usize,
    ) -> Self {
        Self {
            local_attachment_id,
            metadata_id,
            timestamp: Utc::now().timestamp(),
            state: DraftAttachmentUploadState::Uploading,
            action_id: None,
            error: None,
            ownership: DraftAttachmentOwnership::Owned,
            display_order,
            row_id: None,
            deleted: false,
        }
    }

    /// Create a new pending attachment.
    ///
    /// Pending attachments are attachments that automatically trigger upload actions
    /// when the draft is saved.
    pub fn pending(
        metadata_id: MetadataId,
        local_attachment_id: LocalAttachmentId,
        display_order: usize,
    ) -> Self {
        Self {
            local_attachment_id,
            metadata_id,
            timestamp: Utc::now().timestamp(),
            state: DraftAttachmentUploadState::Pending,
            action_id: None,
            error: None,
            ownership: DraftAttachmentOwnership::Owned,
            display_order,
            row_id: None,
            deleted: false,
        }
    }

    /// Create a new inherited `attachment`.
    ///
    /// Inherited attachments are inherited when creating replies or forwarding messages.
    /// The first time we save a draft, they will receive new remote attachment ids.
    pub fn inherited(
        metadata_id: MetadataId,
        attachment: &Attachment,
        display_order: usize,
    ) -> Self {
        Self {
            local_attachment_id: attachment.local_id.unwrap(),
            metadata_id,
            timestamp: Utc::now().timestamp(),
            action_id: None,
            error: None,
            state: DraftAttachmentUploadState::Uploaded,
            ownership: DraftAttachmentOwnership::Inherited,
            display_order,
            row_id: None,
            deleted: false,
        }
    }

    /// Create a new owned attachment that has already been uploaded.
    pub fn owned_and_uploaded(
        metadata_id: MetadataId,
        attachment_id: LocalAttachmentId,
        display_order: usize,
    ) -> Self {
        Self {
            local_attachment_id: attachment_id,
            metadata_id,
            timestamp: Utc::now().timestamp(),
            action_id: None,
            error: None,
            state: DraftAttachmentUploadState::Uploaded,
            ownership: DraftAttachmentOwnership::Owned,
            display_order,
            row_id: None,
            deleted: false,
        }
    }

    /// Overwrite `Model::Save` for create or update.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) = Self::find_by_id(self.local_attachment_id, bond).await? {
            self.row_id = existing.row_id;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Update state.
    fn set_state(&mut self, state: DraftAttachmentUploadState) {
        self.timestamp = Utc::now().timestamp();
        self.state = state;
    }

    /// Update state to error with the given `error`.
    pub fn set_error_state(&mut self, error: DraftAttachmentUploadError) {
        self.set_state(DraftAttachmentUploadState::Error);
        self.error = Some(error);
    }

    /// Update state to uploaded.
    pub fn set_uploaded_state(&mut self) {
        self.set_state(DraftAttachmentUploadState::Uploaded);
        self.error = None;
    }

    /// Update state to uploading.
    pub fn set_uploading_state(&mut self) {
        self.set_state(DraftAttachmentUploadState::Uploading);
    }

    /// Update to offline.
    pub fn set_offline_state(&mut self) {
        self.set_state(DraftAttachmentUploadState::Offline);
    }

    /// Get the current state.
    pub fn state(&self) -> DraftAttachmentUploadState {
        self.state
    }

    /// Timestamp of the latest state update.
    pub fn state_timestamp(&self) -> i64 {
        self.timestamp
    }

    /// Return all [`ActionId`]s for attachments that are still uploading.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn find_attachment_upload_action_ids(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<ActionId>, StashError> {
        tether
            .query_values(
                format!(
                    "SELECT action_id AS value FROM {} WHERE metadata_id = ? AND action_id IS NOT NULL",
                    Self::table_name()
                ),
                params![metadata_id],
            )
            .await
    }

    /// Check whether this draft has attachments that have not been uploaded yet.
    pub async fn has_unsynced_attachments(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        let count = tether
            .query_value::<_, usize>(
                format!(
                    "SELECT COUNT(*) AS value FROM {} WHERE metadata_id = ? AND state <> ?",
                    Self::table_name()
                ),
                params![metadata_id, DraftAttachmentUploadState::Uploaded],
            )
            .await?;
        Ok(count != 0)
    }

    /// Subscribe to changes made to this database table.
    ///
    /// # Errors
    ///
    /// Returns error if the subscription failed.
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(DraftAttachmentMetadataTableObserver { sender }))
    }

    /// Get all metadata for a given `metadata_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn find_by_metadata_id(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Self::find("WHERE metadata_id = ?", params![metadata_id], tether).await
    }

    /// Find all attachments associated with the draft with `metadata_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn attachment_for_draft(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<Attachment>, StashError> {
        Attachment::find(
            formatdoc! {"
              JOIN {} ON {}.local_attachment_id = {}.local_id
              WHERE {}.metadata_id = ?
              ORDER BY {}.display_order ASC
        ", Self::table_name(), Self::table_name(), Attachment::table_name(), Self::table_name(), Self::table_name()},
            params![metadata_id],
            tether,
        )
            .await
    }

    /// Find the metadata for an attachment with a given `content_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn find_with_content_id(
        metadata_id: MetadataId,
        content_id: ContentId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first(formatdoc! {"
            JOIN attachments ON attachments.local_id = {}.local_attachment_id AND attachments.content_id =?
            WHERE {}.metadata_id = ?
        ", Self::table_name(), Self::table_name()},
                         params![content_id,metadata_id],
                         tether,
        ).await
    }

    /// Reset the tracked attachment state after a draft has been synced from the server.
    ///
    /// Reset deletes all existing records and creates new ones that reflect the new state.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn reset_draft_attachments_after_sync(
        metadata_id: MetadataId,
        body_metadata: &MessageBodyMetadata,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let new_state = body_metadata
            .attachments
            .iter()
            .enumerate()
            .map(|(order, a)| DraftAttachmentMetadata {
                local_attachment_id: a.local_id.unwrap(),
                metadata_id,
                timestamp: Utc::now().timestamp(),
                state: DraftAttachmentUploadState::Uploaded,
                ownership: DraftAttachmentOwnership::Owned,
                error: None,
                action_id: None,
                display_order: order,
                row_id: None,
                deleted: false,
            })
            .collect::<Vec<_>>();

        bond.execute(
            formatdoc! {"DELETE FROM {} WHERE metadata_id = ? AND state NOT IN (?,?)", Self::table_name()},
            params![metadata_id, DraftAttachmentUploadState::Error, DraftAttachmentUploadState::Offline],
        )
            .await
            .inspect_err(|e| error!("Failed to delete existing draft metadata records: {e:?}"))?;
        for mut state in new_state {
            state
                .save(bond)
                .await
                .inspect_err(|e| error!("Failed to save attachment metadata: {e:?}"))?;
        }

        Ok(())
    }

    /// Get the next display order index for a new attachment.
    pub async fn next_display_order(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<usize, StashError> {
        tether.query_value::<_, usize>(formatdoc! {"SELECT IFNULL(MAX(display_order),0) AS value FROM {} WHERE metadata_id = ?", Self::table_name()}, params![metadata_id]).await
    }

    /// Get the attachments id of attachments for a draft with `metadata_id` which
    /// are in the pending state.
    ///
    /// # Errors
    ///
    /// Returns error on failure
    ///
    pub async fn pending_attachments(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<LocalAttachmentId>, StashError> {
        tether.query_values(formatdoc! {"SELECT local_attachment_id AS value FROM {} WHERE metadata_id = ? AND state =?", Self::table_name()}, params![metadata_id, DraftAttachmentUploadState::Pending]).await
    }
}

/// Contains the state of the attachment.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum DraftAttachmentUploadState {
    /// Attachment has not been uploaded.
    Uploading = 0,
    /// Attachment has been uploaded to the server
    Uploaded = 1,
    /// Attachment failed to upload or encrypt.
    Error = 2,
    /// Could not upload due to lack of network,
    Offline = 3,
    /// This attachment needs an upload triggered by a save action.
    Pending = 4,
}

impl ToSql for DraftAttachmentUploadState {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for DraftAttachmentUploadState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            0 => Ok(DraftAttachmentUploadState::Uploading),
            1 => Ok(DraftAttachmentUploadState::Uploaded),
            2 => Ok(DraftAttachmentUploadState::Error),
            3 => Ok(DraftAttachmentUploadState::Offline),
            4 => Ok(DraftAttachmentUploadState::Pending),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}

/// Contains the state of the attachment.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum DraftAttachmentOwnership {
    /// Inherited from a reply or forward action.
    ///
    /// When we save a draft with inherited attachments the first time, they receive new
    /// identifiers and we must transition those into an owned version afterwards.
    Inherited = 0,
    /// This is an attachment that we have full control over.
    #[default]
    Owned = 1,
}

impl ToSql for DraftAttachmentOwnership {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for DraftAttachmentOwnership {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            0 => Ok(Self::Inherited),
            1 => Ok(Self::Owned),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}

/// Possible attachment upload errors that are recorded.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum DraftAttachmentUploadError {
    /// Cryptography failure
    Crypto(String),
    /// Message has too many attachments.
    TooManyAttachments,
    /// The message was already sent.
    MessageAlreadySent,
    /// Server replied with error that we are not aware of.
    Server(String),
    /// Unexpected internal error
    Unexpected,
}

sql_using_serde!(DraftAttachmentUploadError);

impl DraftAttachmentUploadError {
    /// Create a new instance from a [`MailContextError`]
    pub fn from_mail_context_error(error: &MailContextError) -> Self {
        match error {
            MailContextError::Api(e) => Self::Server(e.to_string()),
            MailContextError::Draft(Error::Attachment(AttachmentError::MessageAlreadySent)) => {
                Self::MessageAlreadySent
            }
            MailContextError::Draft(Error::Attachment(AttachmentError::TooManyAttachments)) => {
                Self::TooManyAttachments
            }
            MailContextError::Draft(Error::Attachment(AttachmentError::Crypto(e))) => {
                Self::Crypto(e.to_string())
            }
            MailContextError::AttachmentEncryption(e) => Self::Crypto(e.to_string()),
            _ => Self::Unexpected,
        }
    }
}

struct DraftAttachmentMetadataTableObserver {
    sender: flume::Sender<()>,
}

impl TableObserver for DraftAttachmentMetadataTableObserver {
    fn tables(&self) -> Vec<String> {
        vec![DraftAttachmentMetadata::table_name().to_owned()]
    }

    fn on_tables_changed(&self, _: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for DraftAttachmentTableObserver: {}",
                    e
                )
            })
            .ok();
    }
}
