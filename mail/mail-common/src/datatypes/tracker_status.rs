use derive_more::derive::TryFrom;
use stash::exports::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Default, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum TrackerStatus {
    #[default]
    Unknown = 0,
    NoTrackers = 1,
    Trackers = 2,
}

impl FromSql for TrackerStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for TrackerStatus {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
