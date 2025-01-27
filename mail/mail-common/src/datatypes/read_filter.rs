use derive_more::derive::TryFrom;
use stash::exports::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};

/// Conversation and message read filter.
#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, Copy, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum ReadFilter {
    /// Return all messages/conversations.
    #[default]
    All = 0,
    /// Return only unread messages/conversations.
    Unread = 1,
    /// Return only read messages/conversations.
    Read = 2,
}

impl From<Option<bool>> for ReadFilter {
    fn from(value: Option<bool>) -> Self {
        match value {
            Some(unread) => {
                if unread {
                    Self::Unread
                } else {
                    Self::Read
                }
            }
            None => Self::All,
        }
    }
}
impl From<ReadFilter> for Option<bool> {
    fn from(value: ReadFilter) -> Self {
        match value {
            ReadFilter::All => None,
            ReadFilter::Unread => Some(true),
            ReadFilter::Read => Some(false),
        }
    }
}

impl ToSql for ReadFilter {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for ReadFilter {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}
