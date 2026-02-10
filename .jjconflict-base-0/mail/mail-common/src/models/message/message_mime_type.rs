use crate::datatypes::MimeType;
use proton_crypto_account::keys::EmailMimeType;
use serde::{Deserialize, Serialize};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageMimeType {
    #[default]
    TextHtml,
    TextPlain,
}

impl MessageMimeType {
    pub fn from_api(api: MimeType, if_encrypted: impl FnOnce() -> MessageMimeType) -> Self {
        match api {
            MimeType::TextHtml => Self::TextHtml,
            MimeType::TextPlain => Self::TextPlain,
            MimeType::MultipartMixed => if_encrypted(),

            _ => {
                #[cfg(debug_assertions)]
                panic!("Unexpected mime type: {api:?}");

                #[cfg(not(debug_assertions))]
                Self::TextPlain
            }
        }
    }
}

impl From<MessageMimeType> for MimeType {
    fn from(value: MessageMimeType) -> Self {
        match value {
            MessageMimeType::TextHtml => Self::TextHtml,
            MessageMimeType::TextPlain => Self::TextPlain,
        }
    }
}

impl From<MessageMimeType> for EmailMimeType {
    fn from(value: MessageMimeType) -> Self {
        match value {
            MessageMimeType::TextHtml => Self::Html,
            MessageMimeType::TextPlain => Self::Text,
        }
    }
}

impl ToSql for MessageMimeType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Text(match self {
            Self::TextHtml => "text/html".into(),
            Self::TextPlain => "text/plain".into(),
        })))
    }
}

impl FromSql for MessageMimeType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "text/html" => Ok(Self::TextHtml),
            "text/plain" => Ok(Self::TextPlain),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}
