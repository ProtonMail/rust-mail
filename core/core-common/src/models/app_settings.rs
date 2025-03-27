use chrono::Utc;
use derive_more::derive::TryFrom;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};

#[derive(Debug, Clone, PartialEq, Model, Default)]
#[TableName("app_settings")]
pub struct AppSettings {
    #[IdField]
    pub local_id: SingleEntryId,
    #[DbField]
    pub appearance: AppAppearance,
    #[DbField]
    pub protection: AppProtection,
    #[DbField]
    pub auto_lock: ProtectionAutoLock,
    #[DbField]
    pub use_combine_contacts: bool,
    #[DbField]
    pub use_alternative_routing: bool,
    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl AppSettings {
    /// Get the app settings from database
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(SingleEntryId, tether).await
    }

    /// Save or update a app setting.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is updated correctly in the database.
    ///
    /// This method ensures that there is only one mail setting in the table.
    /// Otherwise, it overwrites old record.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // // Make sure there will be only one row.
        if let Some(existing) = Self::get(bond).await? {
            self.row_id = existing.row_id;
            self.local_id = SingleEntryId;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Get the mail settings from database, fallback on default
    pub async fn get_or_default(tether: &Tether) -> Self {
        Self::get(tether)
            .await
            .unwrap_or_default()
            .unwrap_or_default()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum AppAppearance {
    #[default]
    System = 0,
    DarkMode = 1,
    LightMode = 2,
}

impl FromSql for AppAppearance {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for AppAppearance {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum AppProtection {
    #[default]
    None = 0,
    Biometrics = 1,
    Pin = 2,
}

impl FromSql for AppProtection {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for AppProtection {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum ProtectionAutoLock {
    #[default]
    Always,
    Minutes(u8),
}

impl From<u8> for ProtectionAutoLock {
    fn from(value: u8) -> Self {
        if value > 0 {
            Self::Minutes(value)
        } else {
            Self::Always
        }
    }
}

impl From<ProtectionAutoLock> for u8 {
    fn from(value: ProtectionAutoLock) -> Self {
        match value {
            ProtectionAutoLock::Always => 0,
            ProtectionAutoLock::Minutes(val) => val,
        }
    }
}

impl FromSql for ProtectionAutoLock {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Ok(Self::from(val))
    }
}

#[allow(clippy::cast_lossless)]
impl ToSql for ProtectionAutoLock {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(u8::from(*self) as i64)))
    }
}

#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("pin_protection")]
pub struct PinProtection {
    #[IdField]
    pub local_id: SingleEntryId,
    #[DbField]
    pub attempts: u8,
    #[DbField]
    pub last_access_unixepoch: i64,
    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

#[allow(clippy::new_without_default)]
impl PinProtection {
    #[must_use]
    pub fn new() -> Self {
        Self {
            local_id: SingleEntryId,
            attempts: 0,
            last_access_unixepoch: Utc::now().timestamp(),
            row_id: None,
        }
    }

    /// Get the pin protection from database
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(SingleEntryId, tether).await
    }

    /// Save or update a pin protection;
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is updated correctly in the database.
    ///
    /// This method ensures that there is only one mail setting in the table.
    /// Otherwise, it overwrites old record.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // // Make sure there will be only one row.
        if let Some(existing) = Self::get(bond).await? {
            self.row_id = existing.row_id;
            self.local_id = SingleEntryId;
        }

        <Self as Model>::save(self, bond).await
    }
}

// TODO: Add a common way to create single entry table ids

/// An error during SQL deserialization.
/// It means we expected [`MAGIC_ID`] but got {0}
#[derive(Debug, thiserror::Error)]
#[error("Expected constant {expected} local id but got {got}")]
struct NotAMagicLocalIdError {
    expected: u32,
    got: u32,
}

// Mail settings local id. This is a special value that ALWAYS must be equal the constant
/// [`MAGIC_ID`]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct SingleEntryId;

impl SingleEntryId {
    const MAGIC_ID: u32 = 1;
}

impl FromSql for SingleEntryId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let got = u32::from(u8::column_result(value)?);
        if got != Self::MAGIC_ID {
            return Err(FromSqlError::Other(Box::new(NotAMagicLocalIdError {
                expected: Self::MAGIC_ID,
                got,
            })));
        }
        Ok(Self)
    }
}

impl ToSql for SingleEntryId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(i64::from(
            Self::MAGIC_ID,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(0, ProtectionAutoLock::Always)]
    #[test_case(1, ProtectionAutoLock::Minutes(1))]
    #[test_case(60, ProtectionAutoLock::Minutes(60))]
    fn test_from_u8_for_protection_auto_lock(val: u8, expected: ProtectionAutoLock) {
        assert_eq!(ProtectionAutoLock::from(val), expected);
    }

    #[test_case(ProtectionAutoLock::Always, 0)]
    #[test_case(ProtectionAutoLock::Minutes(1), 1)]
    #[test_case(ProtectionAutoLock::Minutes(60), 60)]
    fn test_from_protection_auto_lock_for_u8(val: ProtectionAutoLock, expected: u8) {
        assert_eq!(u8::from(val), expected);
    }
}
