use crate::datatypes::LocalAttachmentId;
use crate::models::{DraftAttachmentInternalError, DraftAttachmentInternalUploadError};
use proton_sqlite3::Migration;
use stash::macros::DbRecord;
use stash::stash::{Bond, StashError};
use stash::{UserDb, params};

#[derive(DbRecord, Debug, Clone, Eq, PartialEq)]
struct V1Value {
    #[DbField]
    local_attachment_id: LocalAttachmentId,
    #[DbField]
    error: DraftAttachmentInternalUploadError,
}

pub struct DraftSendResultAttachmentErrorsMigration;

#[async_trait::async_trait]
impl Migration<UserDb> for DraftSendResultAttachmentErrorsMigration {
    fn name(&self) -> &str {
        "v045_proton_mail_draft_send_result_attachment_errors"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        // Convert any old draft send failures into the new
        let results = tx
            .query::<_, V1Value>(
                "SELECT local_attachment_id, error FROM draft_attachment_metadata WHERE error IS NOT NULL",
                vec![],
            )
            .await?;

        for result in results {
            let new_error = DraftAttachmentInternalError::Upload(result.error);
            tx.execute(
                "UPDATE draft_attachment_metadata SET error=? WHERE local_attachment_id =?",
                params![new_error, result.local_attachment_id],
            )
            .await?;
        }

        Ok(())
    }
}
