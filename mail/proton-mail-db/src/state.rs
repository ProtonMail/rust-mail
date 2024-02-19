use proton_sqlite3::rusqlite::types::{
    FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef,
};
use proton_sqlite3::rusqlite::ToSql;

/// Represents the current "liveliness" of a given resource.
#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum DeletedState {
    /// Resource is alive.
    None = 0,
    /// Resource has been deleted locally, but is still not deleted remotely.
    Local = 1,
    /// Resource has been deleted remotely can now be pruned.
    Remote = 2,
}

impl FromSql for DeletedState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(DeletedState::None),
            1 => Ok(DeletedState::Local),
            2 => Ok(DeletedState::Remote),
            x => Err(FromSqlError::OutOfRange(x as i64)),
        }
    }
}

impl ToSql for DeletedState {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
