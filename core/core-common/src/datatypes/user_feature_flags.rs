use derive_more::TryFrom;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum UserFeatureFlagSource {
    /// Propertiary self-hosted feature flags API
    Unleash = 0,
    /// Our own home-made legacy feature flags
    Legacy = 1,
}

impl FromSql for UserFeatureFlagSource {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for UserFeatureFlagSource {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
