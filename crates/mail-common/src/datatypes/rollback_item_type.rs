use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum RollbackItemType {
    Label = 1,
    Message = 2,
    Conversation = 3,
}

impl FromSql for RollbackItemType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Label),
            2 => Ok(Self::Message),
            3 => Ok(Self::Conversation),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for RollbackItemType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
