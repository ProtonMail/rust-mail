//! Modifies `v001_proton_mail_default_labels`

use crate::models::{
    DraftSendFailure, DraftSendFailureAttachment, DraftSendFailureSave, DraftSendFailureSend,
};
use proton_core_api::services::proton::AddressId;
use proton_sqlite3::Migration;
use stash::macros::DbRecord;
use stash::stash::{Bond, StashError};
use stash::{UserDb, params, sql_using_serde};

pub struct DraftSendResultMigration;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
enum OldDraftSendFailure {
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

impl From<OldDraftSendFailure> for DraftSendFailure {
    fn from(value: OldDraftSendFailure) -> Self {
        match value {
            OldDraftSendFailure::NoRecipients => Self::Send(DraftSendFailureSend::NoRecipients),
            OldDraftSendFailure::AddressDoesNotHavePrimaryKey(v) => {
                Self::Save(DraftSendFailureSave::AddressDoesNotHavePrimaryKey(v))
            }
            OldDraftSendFailure::RecipientEmailInvalid(v) => {
                Self::Send(DraftSendFailureSend::RecipientEmailInvalid(v.into()))
            }
            OldDraftSendFailure::ProtonRecipientDoesNotExist(v) => {
                Self::Send(DraftSendFailureSend::ProtonRecipientDoesNotExist(v.into()))
            }
            OldDraftSendFailure::AddressDisabled(v) => {
                Self::Save(DraftSendFailureSave::AddressDisabled(v))
            }
            OldDraftSendFailure::MessageAlreadySent => {
                Self::Save(DraftSendFailureSave::AlreadySent)
            }
            OldDraftSendFailure::UnknownRecipientValidationError(_) => Self::Internal,
            OldDraftSendFailure::PackageError(_) => Self::Internal,
            OldDraftSendFailure::MessageUpdateIsNotDraft => {
                Self::Save(DraftSendFailureSave::MessageUpdateIsNotDraft)
            }
            OldDraftSendFailure::MessageDoesNotExist => {
                Self::Save(DraftSendFailureSave::MessageDoesNotExist)
            }
            OldDraftSendFailure::NoConnection => Self::NoConnection,
            OldDraftSendFailure::AlreadySent => Self::Save(DraftSendFailureSave::AlreadySent),
            OldDraftSendFailure::AttachmentUpload(v) => {
                // We have no way to port this over to the new setup so just pass in the value as is.
                Self::Attachment(DraftSendFailureAttachment::Other(v))
            }
            OldDraftSendFailure::Server(v) => Self::Server(v),
            OldDraftSendFailure::Internal => Self::Internal,
        }
    }
}

sql_using_serde!(OldDraftSendFailure);
#[derive(DbRecord, Debug, Clone, Eq, PartialEq)]
struct V1Value {
    #[DbField]
    local_message_id: u64,
    #[DbField]
    error: OldDraftSendFailure,
}

#[async_trait::async_trait]
impl Migration<UserDb> for DraftSendResultMigration {
    fn name(&self) -> &str {
        "v019_proton_mail_draft_send_result_refactor"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        // Convert any old draft send failures into the new
        let results = tx
            .query::<_, V1Value>(
                "SELECT local_message_id, error FROM draft_send_result WHERE error IS NOT NULL",
                vec![],
            )
            .await?;

        for result in results {
            let new_error = DraftSendFailure::from(result.error);
            tx.execute(
                "UPDATE draft_send_result SET error=? WHERE local_message_id=?",
                params![new_error, result.local_message_id],
            )
            .await?;
        }

        Ok(())
    }
}
