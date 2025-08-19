use super::MessageMimeType;
use crate::datatypes::LocalMessageId;
use indoc::indoc;
use stash::{
    macros::DbRecord,
    params,
    stash::{Bond, StashError, Tether},
};
use tracing::instrument;

#[derive(Clone, Debug, PartialEq, Eq, DbRecord)]
pub struct MessageBody {
    #[DbField]
    pub body: String,

    #[DbField]
    pub mime_type: MessageMimeType,

    #[DbField]
    pub decryption_error: Option<String>,
}

impl MessageBody {
    pub fn ok(body: impl Into<String>, mime_type: MessageMimeType) -> Self {
        Self {
            body: body.into(),
            mime_type,
            decryption_error: None,
        }
    }

    pub fn html(body: impl Into<String>) -> Self {
        Self::ok(body, MessageMimeType::TextHtml)
    }

    pub fn plain(body: impl Into<String>) -> Self {
        Self::ok(body, MessageMimeType::TextPlain)
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn load(
        id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<MessageBody>, StashError> {
        let rows = tether
            .query(
                indoc! {"
                    SELECT body, mime_type, decryption_error
                    FROM message_body
                    WHERE message_id = ?
                "},
                params![id],
            )
            .await?;

        Ok(rows.into_iter().next())
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn store(&self, id: LocalMessageId, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            indoc! {"
                INSERT INTO message_body (
                    message_id,
                    body,
                    mime_type,
                    decryption_error
                )
                VALUES (?, ?, ?, ?)
                ON CONFLICT (message_id)
                DO UPDATE SET
                    body = ?,
                    mime_type = ?,
                    decryption_error = ?
                "},
            params![
                id,
                self.body.clone(),
                self.mime_type,
                self.decryption_error.clone(),
                self.body.clone(),
                self.mime_type,
                self.decryption_error.clone()
            ],
        )
        .await?;

        Ok(())
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn delete(id: LocalMessageId, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute("DELETE FROM message_body WHERE message_id = ?", params![id])
            .await?;

        Ok(())
    }
}
