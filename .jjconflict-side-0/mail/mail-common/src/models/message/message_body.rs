use crate::datatypes::LocalMessageId;
use indoc::indoc;
use proton_crypto_inbox::message::RawDecryptedBody;
use proton_sqlite3::rusqlite;
use proton_sqlite3::rusqlite::types::ToSqlOutput;
use rusqlite::types;
use stash::exports::{FromSql, FromSqlError, ToSql, Value};
use stash::{
    macros::DbRecord,
    params,
    stash::{Bond, StashError, Tether},
};
use tracing::instrument;
use types::{FromSqlResult, ValueRef};

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RawMessageBodyType {
    Plain = 0,
    Mime = 1,
}

impl ToSql for RawMessageBodyType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for RawMessageBodyType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            0 => Ok(RawMessageBodyType::Plain),
            1 => Ok(RawMessageBodyType::Mime),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, DbRecord)]
pub struct RawMessageBody {
    #[DbField]
    body: Vec<u8>,

    #[DbField]
    signatures: Vec<u8>,

    #[DbField]
    raw_message_id: Option<String>,

    #[DbField]
    decryption_error: Option<String>,

    #[DbField]
    raw_type: RawMessageBodyType,
}

impl RawMessageBody {
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn ok(body: RawDecryptedBody) -> Self {
        match body {
            RawDecryptedBody::Plain {
                raw_body,
                signatures,
            } => Self {
                body: raw_body,
                signatures,
                raw_message_id: None,
                decryption_error: None,
                raw_type: RawMessageBodyType::Plain,
            },
            RawDecryptedBody::Mime {
                message_id,
                raw_body,
                signatures,
            } => Self {
                body: raw_body,
                signatures,
                raw_message_id: Some(message_id),
                decryption_error: None,
                raw_type: RawMessageBodyType::Mime,
            },
        }
    }

    // Note that signatures for drafts only become available after we generate the encrypted
    // content that needs to be uploaded to the server. So they are stored as empty vec in
    // the db until then.
    pub fn local_draft(body: impl Into<String>) -> Self {
        Self::ok(RawDecryptedBody::Plain {
            raw_body: body.into().into_bytes(),
            signatures: vec![],
        })
    }

    pub fn error(body: Vec<u8>, error: impl Into<String>) -> Self {
        Self {
            body,
            signatures: vec![],
            raw_message_id: None,
            decryption_error: Some(error.into()),
            raw_type: RawMessageBodyType::Plain,
        }
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn load(id: LocalMessageId, tether: &Tether) -> Result<Option<Self>, StashError> {
        let rows = tether
            .query(
                indoc! {"
                    SELECT body, signatures, raw_message_id, decryption_error, raw_type
                    FROM raw_message_body
                    WHERE message_id = ?
                "},
                params![id],
            )
            .await?;

        Ok(rows.into_iter().next())
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn store(&self, id: LocalMessageId, tx: &Bond<'_>) -> Result<(), StashError> {
        self.clone().store_and_consume(id, tx).await
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn store_and_consume(
        self,
        id: LocalMessageId,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            indoc! {"
                INSERT INTO raw_message_body (
                    message_id,
                    body,
                    signatures,
                    raw_message_id,
                    decryption_error,
                    raw_type
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT (message_id)
                DO UPDATE SET
                    body = excluded.body,
                    signatures = excluded.signatures,
                    decryption_error = excluded.decryption_error,
                    raw_message_id = excluded.raw_message_id,
                    raw_type = excluded.raw_type
                "},
            params![
                id,
                self.body,
                self.signatures,
                self.raw_message_id,
                self.decryption_error,
                self.raw_type
            ],
        )
        .await?;

        Ok(())
    }

    #[instrument(skip_all, fields(id=%id))]
    pub async fn delete(id: LocalMessageId, tx: &Bond<'_>) -> Result<(), StashError> {
        tx.execute(
            "DELETE FROM raw_message_body WHERE message_id = ?",
            params![id],
        )
        .await?;

        Ok(())
    }

    pub async fn update_signatures(
        id: LocalMessageId,
        signatures: Vec<u8>,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            "UPDATE raw_message_body SET signatures = ? WHERE message_id = ?",
            params![signatures, id],
        )
        .await?;
        Ok(())
    }

    pub fn into_raw_decrypted_body(
        self,
    ) -> Result<RawDecryptedBody, RawMessageBodyDecryptionError> {
        if let Some(error) = self.decryption_error {
            Err(RawMessageBodyDecryptionError {
                error,
                body: String::from_utf8(self.body).unwrap_or(String::from("Invalid utf8")),
            })
        } else {
            Ok(match self.raw_type {
                RawMessageBodyType::Plain => RawDecryptedBody::Plain {
                    raw_body: self.body,
                    signatures: self.signatures,
                },
                RawMessageBodyType::Mime => RawDecryptedBody::Mime {
                    raw_body: self.body,
                    signatures: self.signatures,
                    message_id: self.raw_message_id.expect("Should be set"),
                },
            })
        }
    }

    #[must_use]
    pub fn decryption_failed(&self) -> bool {
        self.decryption_error.is_some()
    }
}

pub struct RawMessageBodyDecryptionError {
    pub error: String,
    pub body: String,
}
