#[cfg(test)]
#[path = "../tests/models/draft_metadata.rs"]
mod draft_metadata;

use crate::MailContextError;
use crate::datatypes::LocalMessageId;
use crate::datatypes::attachment::ContentId;
use crate::datatypes::{LocalAttachmentId, LocalConversationId};
use crate::draft::send::EoData;
use crate::draft::{
    AttachmentDispositionSwapError, AttachmentUploadError, DraftExpirationTime, Error,
    PackageError, PasswordError, ReplyMode, SaveError, SendError,
};
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::errors::{
    DraftAttachmentDispositionSwapErrorReason, DraftAttachmentUploadErrorReason,
    DraftSaveErrorReason, DraftSendErrorReason, MailErrorReason, ProtonMailError,
};
use crate::models::{Attachment, Message, MessageBodyMetadata};
use anyhow::anyhow;
use chrono::Utc;
use derive_more::derive::TryFrom;
use indoc::formatdoc;
use proton_action_queue::action::ActionId;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{AddressId, PrivateEmail};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::db::account::{EncryptedPassword, SessionEncryptionKey};
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::SqliteError;
use stash::exports::*;
use stash::macros::{DbRecord, Model};
use stash::orm::{Model, ModelHooks};
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

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub local_message_id: Option<LocalMessageId>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub local_conversation_id: Option<LocalConversationId>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub local_parent_id: Option<LocalMessageId>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub reply_mode: Option<ReplyMode>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub save_action_id: Option<ActionId>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub send_action_id: Option<ActionId>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    expiration_time: Option<UnixTimestamp>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub password: Option<EncryptedPassword>,

    #[builder(default, setter(strip_option))]
    #[DbField]
    pub password_hint: Option<String>,

    #[DbField]
    #[builder(default)]
    expiration_option: DraftExpirationOption,
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
            expiration_time: None,
            password: None,
            password_hint: None,
            expiration_option: DraftExpirationOption::Never,
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
        expiration_time: Option<UnixTimestamp>,
        bond: &Bond<'_>,
    ) -> Result<Self, StashError> {
        let expiration_option = if expiration_time.is_none() {
            DraftExpirationOption::Never
        } else {
            DraftExpirationOption::Custom
        };
        let mut metadata = Self {
            id: None,
            local_message_id: None,
            local_conversation_id: Some(source_conversation_id),
            local_parent_id: Some(source_message_id),
            reply_mode: Some(reply_mode),
            send_action_id: None,
            save_action_id: None,
            expiration_time,
            password: None,
            password_hint: None,
            expiration_option,
        };

        metadata.save(bond).await?;

        Ok(metadata)
    }
    pub fn with_ids(message_id: LocalMessageId, conversation_id: LocalConversationId) -> Self {
        Self {
            id: None,
            local_message_id: Some(message_id),
            local_conversation_id: Some(conversation_id),
            local_parent_id: None,
            reply_mode: None,
            send_action_id: None,
            save_action_id: None,
            expiration_time: None,
            password: None,
            password_hint: None,
            expiration_option: DraftExpirationOption::Never,
        }
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
        tether
            .sync_query(move |conn| Self::find_by_message_id_sync(local_message_id, conn))
            .await
    }

    pub fn find_by_message_id_sync(
        local_message_id: LocalMessageId,
        conn: &Connection,
    ) -> Result<Option<Self>, StashError> {
        DraftMetadata::find_first_sync("WHERE local_message_id=?", (local_message_id,), conn)
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

    pub async fn find_by_message_with_remote_id(
        remote_id: MessageId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        let Some(local_id) = Message::remote_id_counterpart(remote_id, tether).await? else {
            return Ok(None);
        };
        Self::find_by_message_id(local_id, tether).await
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
                    "SELECT save_action_id FROM {} WHERE id =?",
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

    #[allow(clippy::result_large_err)]
    pub fn to_eo_data(
        &self,
        session_encryption_key: &SessionEncryptionKey,
    ) -> Result<Option<EoData>, MailContextError> {
        let Some(password) = self.password.as_ref() else {
            return Ok(None);
        };
        let password = SecretString::from(
            String::from_utf8(
                session_encryption_key
                    .decrypt(password.as_ref())
                    .map_err(|_| PasswordError::Decryption)?,
            )
            .map_err(|_| MailContextError::Other(anyhow!("Draft password is not valid utf8")))?,
        );
        Ok(Some(EoData {
            password,
            password_hint: self.password_hint.clone(),
        }))
    }

    pub fn expiration_time(&self) -> DraftExpirationTime {
        match self.expiration_option {
            DraftExpirationOption::Never => DraftExpirationTime::Never,
            DraftExpirationOption::OneHour => DraftExpirationTime::OneHour,
            DraftExpirationOption::OneDay => DraftExpirationTime::OneDay,
            DraftExpirationOption::ThreeDays => DraftExpirationTime::ThreeDays,
            DraftExpirationOption::Custom => self
                .expiration_time
                .map(|v| {
                    v.to_date_time()
                        .map(DraftExpirationTime::Custom)
                        .unwrap_or(DraftExpirationTime::Never)
                })
                .unwrap_or(DraftExpirationTime::Never),
        }
    }

    pub fn set_expiration_time(&mut self, expiration: DraftExpirationTime) {
        match expiration {
            DraftExpirationTime::Never => {
                self.expiration_time = None;
                self.expiration_option = DraftExpirationOption::Never;
            }
            DraftExpirationTime::OneHour => {
                self.expiration_time = None;
                self.expiration_option = DraftExpirationOption::OneHour;
            }
            DraftExpirationTime::OneDay => {
                self.expiration_time = None;
                self.expiration_option = DraftExpirationOption::OneDay;
            }
            DraftExpirationTime::ThreeDays => {
                self.expiration_time = None;
                self.expiration_option = DraftExpirationOption::ThreeDays;
            }
            DraftExpirationTime::Custom(v) => {
                self.expiration_time = Some(v.into());
                self.expiration_option = DraftExpirationOption::Custom;
            }
        }
    }
}

/// Due to architectural differences on some of the platforms we need to store the
/// result of the send action in the database rather than relying on the queue observers.
#[derive(Clone, Debug, Eq, Model, PartialEq, Hash)]
#[ModelHooks]
#[TableName("draft_send_result")]
pub struct DraftSendResult {
    #[IdField]
    pub local_message_id: LocalMessageId,

    /// Only set when the message was sent successfully.
    #[DbField]
    pub remote_message_id: Option<MessageId>,

    #[DbField]
    pub timestamp: UnixTimestamp,

    /// Timestamp by which we can cancel the sending of this message, this corresponds to
    /// the message delivery time.
    #[DbField]
    pub undo_timestamp: UnixTimestamp,

    #[DbField]
    pub error: Option<DraftSendFailure>,

    #[DbField]
    pub seen: bool,

    #[DbField]
    pub origin: DraftSendResultOrigin,

    #[DbField]
    pub has_send_action: bool,
}

impl ModelHooks for DraftSendResult {
    fn before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        // Only overwrite if present.
        if let Some(metadata_id) =
            DraftMetadata::find_by_message_id_sync(self.local_message_id, tx)?
        {
            self.has_send_action = metadata_id.send_action_id.is_some();
        }
        Ok(())
    }
}

impl DraftSendResult {
    /// Create a new draft send success result for message with `local_message_id` and
    /// the server returned `undo_token`.
    pub fn success(
        local_message_id: LocalMessageId,
        remote_message_id: MessageId,
        undo_timestamp: UnixTimestamp,
        origin: DraftSendResultOrigin,
    ) -> Self {
        Self {
            local_message_id,
            remote_message_id: Some(remote_message_id),
            timestamp: UnixTimestamp::now(),
            undo_timestamp,
            error: None,
            seen: false,
            origin,
            has_send_action: false,
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
            timestamp: UnixTimestamp::now(),
            undo_timestamp: 0.into(),
            seen: false,
            error: Some(error),
            has_send_action: false,
            origin,
        }
    }

    /// Returns all unseen send results.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unseen(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find("WHERE seen=0 ORDER BY timestamp DESC", vec![], tether).await
    }

    pub async fn unseen_with_send_action(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find(
            "WHERE seen=0 AND (has_send_action= 1 OR origin = ?) ORDER BY timestamp DESC",
            params![DraftSendResultOrigin::SaveBeforeSend],
            tether,
        )
        .await
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
                    "SELECT local_message_id FROM `{}` WHERE seen=0",
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
    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(DraftSendResultTableObserver { sender }))
            .await
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
        let now = UnixTimestamp::now();
        now < self.undo_timestamp
    }

    /// Returns the time left until this message's sending can be cancelled.
    #[must_use]
    pub fn time_left_for_undo(&self) -> Duration {
        let now = UnixTimestamp::now();
        Duration::from_secs(self.undo_timestamp.as_u64().saturating_sub(now.as_u64()))
    }
}

/// Represents the reason why a draft failed to send.
///
/// Unfortunately we can not re-use [`DraftSaveSendErrorReason`] as we can not take ownership of
/// the error so we have to do our own conversion.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailure {
    Save(DraftSendFailureSave),
    Send(DraftSendFailureSend),
    Attachment(DraftSendFailureAttachment),
    NoConnection,
    Server(String),
    AttachmentDispositionSwap(DraftSendFailureDispositionSwap),
    Internal,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailureSave {
    AddressDisabled(String),
    AddressDoesNotHavePrimaryKey(AddressId),
    AlreadySent,
    MessageUpdateIsNotDraft,
    MessageDoesNotExist,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailureSend {
    NoRecipients,
    RecipientEmailInvalid(PrivateEmail),
    ProtonRecipientDoesNotExist(PrivateEmail),
    PackageError(String),
    MessageDoesNotExist,
    ScheduleSendExpired,
    ScheduleSendLimitExceeded,
    EOPasswordDecrypt,
    ExpirationTimeTooSoon,
    MissingAttachmentUploads,
    MessageTooLarge,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailureAttachment {
    Crypto(String),
    TooManyAttachments,
    AttachmentTooLarge,
    AttachmentAlreadyUploaded,
    TotalAttachmentsTooLarge,
    MessageDoesNotExist,
    Timeout,
    StorageQuotaExceeded,
    Other(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub enum DraftSendFailureDispositionSwap {
    AttachmentDoesNotExist,
    AttachmentMessagedDoesNotExist,
    AttachmentMessageIsNotADraft,
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
    /// We failed when scheduling a message send
    ScheduleSend = 4,
    /// We failed to swap the disposition on the message
    AttachmentDispositionSwap = 5,
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
        matches!(self, Self::NoConnection)
    }

    /// Convert from a draft [`Error`]
    #[must_use]
    pub fn from_draft_error(error: &Error) -> Self {
        match error {
            Error::Save(err) => match err {
                SaveError::AddressWithoutPrimaryKey(id) => Self::Save(
                    DraftSendFailureSave::AddressDoesNotHavePrimaryKey(id.clone()),
                ),
                SaveError::AlreadySent => Self::Save(DraftSendFailureSave::AlreadySent),
                SaveError::AddressNotFound(_)
                | SaveError::MessageNotADraft(_)
                | SaveError::AttachmentDoesNotHaveKeyPackets(_)
                | SaveError::MetadataNotFound(_)
                | SaveError::DraftDoesNotExistOnServer => Self::Internal,
            },
            Error::Send(err) => match err {
                SendError::SendMessage(package_error) => {
                    Self::from_draft_package_error(package_error)
                }
                SendError::NoRecipients => Self::Send(DraftSendFailureSend::NoRecipients),
                SendError::ScheduleSendExpired => {
                    Self::Send(DraftSendFailureSend::ScheduleSendExpired)
                }
                SendError::ExpirationTimeTooSoon => {
                    Self::Send(DraftSendFailureSend::ExpirationTimeTooSoon)
                }
                SendError::EOPasswordDecrypt => Self::Send(DraftSendFailureSend::EOPasswordDecrypt),
                SendError::MessageIsNotADraft(_)
                | SendError::MetadataNotFound(_)
                | SendError::LocalDraftWithoutMessage
                | SendError::DraftDoesNotExistOnServer
                | SendError::MessageBodyMissing(_) => Self::Internal,
                SendError::ScheduleSendMessageLimitExceeded => {
                    Self::Send(DraftSendFailureSend::ScheduleSendLimitExceeded)
                }
                SendError::MissingAttachmentUploads => {
                    Self::Send(DraftSendFailureSend::MissingAttachmentUploads)
                }
                SendError::MessageTooLarge => Self::Send(DraftSendFailureSend::MessageTooLarge),
            },
            Error::AttachmentUpload(e) => match e {
                AttachmentUploadError::MessageDoesNotExist
                | AttachmentUploadError::MessageDoesNotExistOnServer(_) => {
                    Self::Attachment(DraftSendFailureAttachment::MessageDoesNotExist)
                }
                AttachmentUploadError::Crypto(e) => {
                    Self::Attachment(DraftSendFailureAttachment::Crypto(e.to_string()))
                }
                AttachmentUploadError::AttachmentAlreadyUploaded(_) => {
                    Self::Attachment(DraftSendFailureAttachment::AttachmentAlreadyUploaded)
                }
                AttachmentUploadError::TooManyAttachments => {
                    Self::Attachment(DraftSendFailureAttachment::TooManyAttachments)
                }
                AttachmentUploadError::AttachmentTooLarge => {
                    Self::Attachment(DraftSendFailureAttachment::AttachmentTooLarge)
                }
                AttachmentUploadError::TotalAttachmentSizeTooLarge => {
                    Self::Attachment(DraftSendFailureAttachment::TotalAttachmentsTooLarge)
                }
                AttachmentUploadError::Timeout => {
                    Self::Attachment(DraftSendFailureAttachment::Timeout)
                }
                AttachmentUploadError::MetadataNotFound(_)
                | AttachmentUploadError::AttachmentMetadataNotFound(_)
                | AttachmentUploadError::AttachmentMetadataNotFoundCid(_)
                | AttachmentUploadError::AttachmentDataMissing(_)
                | AttachmentUploadError::MissingContentId(_)
                | AttachmentUploadError::ExistingUploadActionExist(_)
                | AttachmentUploadError::MessageAlreadySent
                | AttachmentUploadError::RetryInvalidState(_) => Self::Internal,
                AttachmentUploadError::StorageQuotaExceeded => {
                    Self::Attachment(DraftSendFailureAttachment::StorageQuotaExceeded)
                }
            },
            Error::AttachmentDispositionSwap(e) => match e {
                AttachmentDispositionSwapError::MetadataNotFound(_)
                | AttachmentDispositionSwapError::AttachmentHasNoRemoteId(_)
                | AttachmentDispositionSwapError::AttachmentMetadataNotFound(_)
                | AttachmentDispositionSwapError::Noop
                | AttachmentDispositionSwapError::NoMessageIdInDraftMetadata(_)
                | AttachmentDispositionSwapError::InvalidState(_)
                | AttachmentDispositionSwapError::AttachmentHasNoContentId(_) => Self::Internal,
                AttachmentDispositionSwapError::AttachmentNotFound(_)
                | AttachmentDispositionSwapError::AttachmentNotFoundCid(_)
                | AttachmentDispositionSwapError::AttachmentDoesNotExistServer(_) => {
                    Self::AttachmentDispositionSwap(
                        DraftSendFailureDispositionSwap::AttachmentDoesNotExist,
                    )
                }
                AttachmentDispositionSwapError::AttachmentMessageDoesNotExist(_) => {
                    Self::AttachmentDispositionSwap(
                        DraftSendFailureDispositionSwap::AttachmentMessagedDoesNotExist,
                    )
                }
                AttachmentDispositionSwapError::AttachmentMessageIsNotADraft(_) => {
                    Self::AttachmentDispositionSwap(
                        DraftSendFailureDispositionSwap::AttachmentMessageIsNotADraft,
                    )
                }
                AttachmentDispositionSwapError::AttachmentDoesNotHaveValidCid(_) => Self::Internal,
            },

            _ => Self::Internal,
        }
    }

    /// Convert from a draft [`PackageError`]
    #[must_use]
    pub fn from_draft_package_error(value: &PackageError) -> Self {
        match value {
            PackageError::RecipientEmailInvalid(e) => {
                Self::Send(DraftSendFailureSend::RecipientEmailInvalid(e.clone()))
            }
            PackageError::ProtonRecipientDoesNotExist(e) => {
                Self::Send(DraftSendFailureSend::ProtonRecipientDoesNotExist(e.clone()))
            }
            v => Self::Send(DraftSendFailureSend::PackageError(v.to_string())),
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
            DraftSendFailure::NoConnection => Self::Network,
            DraftSendFailure::Server(v) => {
                // While there is no good conversion to be performed here, it should be very rare
                // that any error we are interested in handling should slip past here.
                // In those cases the error is still logged completely in the action execution
                // code.
                Self::ServerError(UserApiServiceError::OtherHttpError(0, v))
            }
            DraftSendFailure::Internal => Self::Unexpected(Unexpected::Internal),
            DraftSendFailure::Save(err) => {
                Self::Reason(MailErrorReason::DraftSaveReason(match err {
                    DraftSendFailureSave::AddressDisabled(v) => {
                        DraftSaveErrorReason::AddressDisabled(v)
                    }
                    DraftSendFailureSave::AddressDoesNotHavePrimaryKey(v) => {
                        DraftSaveErrorReason::AddressDoesNotHavePrimaryKey(v)
                    }
                    DraftSendFailureSave::AlreadySent => DraftSaveErrorReason::MessageAlreadySent,
                    DraftSendFailureSave::MessageUpdateIsNotDraft => {
                        DraftSaveErrorReason::MessageIsNotADraft
                    }
                    DraftSendFailureSave::MessageDoesNotExist => {
                        DraftSaveErrorReason::MessageDoesNotExist
                    }
                }))
            }
            DraftSendFailure::Send(err) => {
                Self::Reason(MailErrorReason::DraftSendReason(match err {
                    DraftSendFailureSend::NoRecipients => DraftSendErrorReason::NoRecipients,
                    DraftSendFailureSend::RecipientEmailInvalid(v) => {
                        DraftSendErrorReason::RecipientEmailInvalid(v)
                    }
                    DraftSendFailureSend::ProtonRecipientDoesNotExist(v) => {
                        DraftSendErrorReason::ProtonRecipientDoesNotExist(v)
                    }
                    DraftSendFailureSend::PackageError(v) => DraftSendErrorReason::PackageError(v),
                    DraftSendFailureSend::MessageDoesNotExist => {
                        DraftSendErrorReason::MessageDoesNotExist
                    }
                    DraftSendFailureSend::ScheduleSendExpired => {
                        DraftSendErrorReason::ScheduleSendExpired
                    }
                    DraftSendFailureSend::EOPasswordDecrypt => {
                        DraftSendErrorReason::EOPasswordDecrypt
                    }
                    DraftSendFailureSend::ExpirationTimeTooSoon => {
                        DraftSendErrorReason::ExpirationTimeTooSoon
                    }
                    DraftSendFailureSend::MissingAttachmentUploads => {
                        DraftSendErrorReason::MissingAttachmentUploads
                    }
                    DraftSendFailureSend::MessageTooLarge => DraftSendErrorReason::MessageTooLarge,
                    DraftSendFailureSend::ScheduleSendLimitExceeded => {
                        DraftSendErrorReason::ScheduleSendMessageLimitExceeded
                    }
                }))
            }
            DraftSendFailure::Attachment(err) => match err {
                DraftSendFailureAttachment::Crypto(_) => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::Crypto,
                    ))
                }
                DraftSendFailureAttachment::TooManyAttachments => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::TooManyAttachments,
                    ))
                }
                DraftSendFailureAttachment::AttachmentTooLarge => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::AttachmentTooLarge,
                    ))
                }
                DraftSendFailureAttachment::AttachmentAlreadyUploaded => {
                    Self::Unexpected(Unexpected::Draft)
                }
                DraftSendFailureAttachment::MessageDoesNotExist => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::MessageDoesNotExist,
                    ))
                }
                DraftSendFailureAttachment::Other(_) => Self::Unexpected(Unexpected::Draft),
                DraftSendFailureAttachment::TotalAttachmentsTooLarge => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::TotalAttachmentSizeTooLarge,
                    ))
                }
                DraftSendFailureAttachment::Timeout => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::Timeout,
                    ))
                }
                DraftSendFailureAttachment::StorageQuotaExceeded => {
                    Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                        DraftAttachmentUploadErrorReason::StorageQuotaExceeded,
                    ))
                }
            },
            DraftSendFailure::AttachmentDispositionSwap(e) => match e {
                DraftSendFailureDispositionSwap::AttachmentDoesNotExist => {
                    Self::Reason(MailErrorReason::DraftAttachmentDispositionSwapError(
                        DraftAttachmentDispositionSwapErrorReason::AttachmentDoesNotExist,
                    ))
                }
                DraftSendFailureDispositionSwap::AttachmentMessagedDoesNotExist => {
                    Self::Reason(MailErrorReason::DraftAttachmentDispositionSwapError(
                        DraftAttachmentDispositionSwapErrorReason::AttachmentMessageDoesNotExist,
                    ))
                }
                DraftSendFailureDispositionSwap::AttachmentMessageIsNotADraft => {
                    Self::Reason(MailErrorReason::DraftAttachmentDispositionSwapError(
                        DraftAttachmentDispositionSwapErrorReason::AttachmentMessageIsNotADraft,
                    ))
                }
            },
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
    #[IdField]
    pub local_attachment_id: LocalAttachmentId,

    #[DbField]
    pub metadata_id: MetadataId,

    #[DbField]
    timestamp: i64,

    #[DbField]
    state: DraftAttachmentUploadState,

    #[DbField]
    pub ownership: DraftAttachmentOwnership,

    #[DbField]
    pub error: Option<DraftAttachmentInternalError>,

    #[DbField]
    pub display_order: usize,

    #[DbField]
    pub action_id: Option<ActionId>,

    #[DbField]
    pub deleted: bool,

    #[DbField]
    pub is_public_key: bool,
}

impl DraftAttachmentMetadata {
    /// Create a new instance
    pub fn new(
        metadata_id: MetadataId,
        local_attachment_id: LocalAttachmentId,
        display_order: usize,
        is_public_key: bool,
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
            deleted: false,
            is_public_key,
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
        is_public_key: bool,
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
            deleted: false,
            is_public_key,
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
            local_attachment_id: attachment.id(),
            metadata_id,
            timestamp: Utc::now().timestamp(),
            action_id: None,
            error: None,
            state: DraftAttachmentUploadState::Uploaded,
            ownership: DraftAttachmentOwnership::Inherited,
            display_order,
            deleted: false,
            is_public_key: attachment.is_public_key_attachment(),
        }
    }

    /// Create a new owned attachment that has already been uploaded.
    pub fn owned_and_uploaded(
        metadata_id: MetadataId,
        attachment_id: LocalAttachmentId,
        display_order: usize,
        is_public_key: bool,
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
            deleted: false,
            is_public_key,
        }
    }

    /// Update state.
    fn set_state(&mut self, state: DraftAttachmentUploadState) {
        self.timestamp = Utc::now().timestamp();
        self.state = state;
    }

    /// Update state to error with the given `error`.
    pub fn set_error_state(&mut self, error: DraftAttachmentInternalError) {
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

    pub fn set_disposition_swap_state(&mut self) {
        self.set_state(DraftAttachmentUploadState::DispositionSwap);
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
                    "SELECT action_id FROM {} WHERE metadata_id = ? AND action_id IS NOT NULL AND deleted = 0",
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
                    "SELECT COUNT(*) FROM {} WHERE metadata_id = ? AND state <> ? AND deleted = 0",
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
    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(DraftAttachmentMetadataTableObserver { sender }))
            .await
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

    pub async fn public_key_attachments(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<Vec<Attachment>, StashError> {
        Attachment::find(
            formatdoc! {"
              JOIN {} ON {}.local_attachment_id = {}.local_id AND {}.is_public_key = 1
              WHERE {}.metadata_id = ?
              ORDER BY {}.display_order ASC
        ", Self::table_name(), Self::table_name(), Attachment::table_name(), Self::table_name(), Self::table_name(), Self::table_name()},
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
                local_attachment_id: a.id(),
                metadata_id,
                timestamp: Utc::now().timestamp(),
                state: DraftAttachmentUploadState::Uploaded,
                ownership: DraftAttachmentOwnership::Owned,
                error: None,
                action_id: None,
                display_order: order,
                deleted: false,
                is_public_key: a.is_public_key_attachment(),
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
        tether.query_value::<_, usize>(formatdoc! {"SELECT IFNULL(MAX(display_order),0) FROM {} WHERE metadata_id = ?", Self::table_name()}, params![metadata_id]).await
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
        tether.query_values(formatdoc! {"SELECT local_attachment_id FROM {} WHERE metadata_id = ? AND state =? AND deleted = 0", Self::table_name()}, params![metadata_id, DraftAttachmentUploadState::Pending]).await
    }

    pub async fn total_attachments_size_and_count(
        metadata_id: MetadataId,
        tether: &Tether,
    ) -> Result<DraftAttachmentsTotalCountAndSize, StashError> {
        Ok(tether.query::<_, DraftAttachmentsTotalCountAndSize>(
            formatdoc! {"
                SELECT IFNULL(COUNT(attachments.local_id),0) AS total, IFNULL(SUM(attachments.size),0) AS total_size
                FROM attachments
                JOIN  {table} ON {table}.local_attachment_id= attachments.local_id AND {table}.metadata_id = ?
                WHERE {table}.deleted = 0
            ",
                table = Self::table_name()
            }, params![metadata_id],
        ).await?.into_iter().next().unwrap_or(DraftAttachmentsTotalCountAndSize {
            total: 0,
            total_size: 0,
        }))
    }

    pub fn is_upload_error(&self) -> bool {
        self.error.as_ref().is_some_and(|e| e.is_upload_error())
    }

    pub fn is_disposition_swap_error(&self) -> bool {
        self.error
            .as_ref()
            .is_some_and(|e| e.is_disposition_swap_error())
    }
}
#[derive(DbRecord, Debug, Eq, PartialEq, Copy, Clone)]
pub struct DraftAttachmentsTotalCountAndSize {
    #[DbField]
    pub total: usize,
    #[DbField]
    pub total_size: u64,
}

/// Contains the state of the attachment.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum DraftAttachmentUploadState {
    Uploading = 0,
    Uploaded = 1,
    Error = 2,
    Offline = 3,
    Pending = 4,
    DispositionSwap = 5,
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
            5 => Ok(DraftAttachmentUploadState::DispositionSwap),
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
pub enum DraftAttachmentInternalError {
    Upload(DraftAttachmentInternalUploadError),
    DispositionSwap(DraftAttachmentInternalDispositionError),
}

impl DraftAttachmentInternalError {
    pub fn from_mail_context_error(
        origin: DraftSendResultOrigin,
        error: &MailContextError,
    ) -> Self {
        match origin {
            DraftSendResultOrigin::AttachmentDispositionSwap => Self::DispositionSwap(
                DraftAttachmentInternalDispositionError::from_mail_context_error(error),
            ),
            DraftSendResultOrigin::AttachmentUpload => Self::Upload(
                DraftAttachmentInternalUploadError::from_mail_context_error(error),
            ),
            _ => {
                unreachable!("Should not be triggered");
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum DraftAttachmentInternalUploadError {
    /// Cryptography failure
    Crypto(String),
    /// Message has too many attachments.
    TooManyAttachments,
    /// The message was already sent.
    MessageAlreadySent,
    /// Server replied with error that we are not aware of.
    Server(String),
    AttachmentTooLarge,
    TotalAttachmentsTooLarge,
    /// Unexpected internal error
    Unexpected,
}

sql_using_serde!(DraftAttachmentInternalUploadError);

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
pub enum DraftAttachmentInternalDispositionError {
    Server(String),
    AttachmentNotFound,
    MessageIsNotDraft,
    MessageDoesNotExist,
    Unexpected,
}

impl DraftAttachmentInternalError {
    fn is_upload_error(&self) -> bool {
        match self {
            DraftAttachmentInternalError::Upload(_) => true,
            DraftAttachmentInternalError::DispositionSwap(_) => false,
        }
    }

    fn is_disposition_swap_error(&self) -> bool {
        match self {
            DraftAttachmentInternalError::Upload(_) => false,
            DraftAttachmentInternalError::DispositionSwap(_) => true,
        }
    }
}

sql_using_serde!(DraftAttachmentInternalError);

impl DraftAttachmentInternalUploadError {
    /// Create a new instance from a [`MailContextError`]
    pub fn from_mail_context_error(error: &MailContextError) -> Self {
        match error {
            MailContextError::Api(e) => Self::Server(e.to_string()),
            MailContextError::Draft(Error::AttachmentUpload(
                AttachmentUploadError::MessageAlreadySent,
            )) => Self::MessageAlreadySent,
            MailContextError::Draft(Error::AttachmentUpload(
                AttachmentUploadError::TooManyAttachments,
            )) => Self::TooManyAttachments,
            MailContextError::Draft(Error::AttachmentUpload(AttachmentUploadError::Crypto(e))) => {
                Self::Crypto(e.to_string())
            }
            MailContextError::AttachmentEncryption(e) => Self::Crypto(e.to_string()),
            MailContextError::Draft(Error::AttachmentUpload(
                AttachmentUploadError::AttachmentTooLarge,
            )) => Self::AttachmentTooLarge,
            MailContextError::Draft(Error::AttachmentUpload(
                AttachmentUploadError::TotalAttachmentSizeTooLarge,
            )) => Self::TotalAttachmentsTooLarge,
            _ => Self::Unexpected,
        }
    }
}

impl DraftAttachmentInternalDispositionError {
    pub fn from_mail_context_error(error: &MailContextError) -> Self {
        match error {
            MailContextError::Api(e) => Self::Server(e.to_string()),
            MailContextError::Draft(Error::AttachmentDispositionSwap(e)) => match e {
                AttachmentDispositionSwapError::AttachmentNotFoundCid(_)
                | AttachmentDispositionSwapError::AttachmentNotFound(_)
                | AttachmentDispositionSwapError::AttachmentDoesNotExistServer(_) => {
                    Self::AttachmentNotFound
                }
                AttachmentDispositionSwapError::AttachmentMessageIsNotADraft(_) => {
                    Self::MessageIsNotDraft
                }
                AttachmentDispositionSwapError::AttachmentMessageDoesNotExist(_) => {
                    Self::MessageDoesNotExist
                }
                AttachmentDispositionSwapError::MetadataNotFound(_)
                | AttachmentDispositionSwapError::AttachmentHasNoRemoteId(_)
                | AttachmentDispositionSwapError::AttachmentMetadataNotFound(_)
                | AttachmentDispositionSwapError::Noop
                | AttachmentDispositionSwapError::NoMessageIdInDraftMetadata(_)
                | AttachmentDispositionSwapError::InvalidState(_)
                | AttachmentDispositionSwapError::AttachmentHasNoContentId(_)
                | AttachmentDispositionSwapError::AttachmentDoesNotHaveValidCid(_) => {
                    Self::Unexpected
                }
            },
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

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum DraftExpirationOption {
    #[default]
    Never = 0,
    OneHour = 1,
    OneDay = 2,
    ThreeDays = 3,
    Custom = 4,
}

impl ToSql for DraftExpirationOption {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::from(*self as u8)))
    }
}

impl FromSql for DraftExpirationOption {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}
