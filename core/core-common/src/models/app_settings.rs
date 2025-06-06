use chrono::Utc;
use derive_more::derive::TryFrom;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};

use crate::datatypes::UnixTimestamp;
use crate::pin_code::PinCode;
use smart_default::SmartDefault;

/// Struct Representing `AppSettings` - cross accounts settings of the application.
///
/// This model is stored in account (shared) database.
///
#[derive(Debug, Clone, PartialEq, Model, SmartDefault)]
#[TableName("app_settings")]
pub struct AppSettings {
    /// There is only one entry of `AppSettings`
    /// stored in database.
    ///
    #[IdField]
    pub local_id: SingleEntryId,

    /// The theme of the Application
    #[DbField]
    pub appearance: AppAppearance,

    /// What additional protection of the app is in use.
    #[DbField]
    pub protection: AppProtection,

    /// Autolock time for additional protection to kick in,
    /// when app is running in bg for extended time.
    #[DbField]
    pub auto_lock: ProtectionAutoLock,

    /// When auto-lock was lastly invoked,
    #[DbField]
    #[default(_code = "UnixTimestamp::new(0)")]
    pub lock_accessed_unixepoch: UnixTimestamp,

    /// Do you want to share contacts between the accounts.
    #[DbField]
    pub use_combine_contacts: bool,

    /// Use alternative routing, helpful for ppl leaving in
    /// area where Proton servers are blocked for any reason.
    #[DbField]
    #[default = true]
    pub use_alternative_routing: bool,

    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl AppSettings {
    pub fn set_biometrics(&mut self) {
        if let AppProtection::None = self.protection {
            self.protection = AppProtection::Biometrics;
            self.lock_accessed_unixepoch = UnixTimestamp::now();
        }
    }

    pub fn unset_biometrics(&mut self) {
        if let AppProtection::Biometrics = self.protection {
            self.protection = AppProtection::None;
        }
    }

    /// Returns information if enough amount of time has passed from the autolock setting.
    ///
    /// Method automatically stores current time when returning `true`, allowing
    /// for repetable calls checking if the time has passed since last autolock.
    ///
    pub async fn should_auto_lock(&mut self, tether: &mut Tether) -> Result<bool, StashError> {
        if self.protection.is_unset() {
            Ok(false)
        } else {
            let now = UnixTimestamp::now();
            let should_lock = self
                .auto_lock
                .should_autolock(now, self.lock_accessed_unixepoch);

            if should_lock {
                self.lock_accessed_unixepoch = now;
                tether.tx(async |bond| self.save(bond).await).await?;
            }

            Ok(should_lock)
        }
    }

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
        // Make sure there will be only one row.
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

/// Representation of available themes for the app.
///
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

/// Supported additional protection for accessing app.
///
#[derive(Debug, Copy, Clone, PartialEq, Default, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum AppProtection {
    #[default]
    None = 0,
    Biometrics = 1,
    Pin = 2,
}

impl AppProtection {
    #[must_use]
    pub fn is_set(&self) -> bool {
        !self.is_unset()
    }

    #[must_use]
    pub fn is_unset(&self) -> bool {
        matches!(self, AppProtection::None)
    }
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

/// How much time till app in the background will require
/// authentication when going to foreground.
///
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ProtectionAutoLock {
    Always,
    Minutes(u8),
    Never,
}

impl Default for ProtectionAutoLock {
    fn default() -> Self {
        Self::Minutes(15)
    }
}

impl ProtectionAutoLock {
    #[must_use]
    pub fn should_autolock(&self, now: UnixTimestamp, last_lock: UnixTimestamp) -> bool {
        match self {
            Self::Always => true,
            Self::Minutes(minutes) => {
                let seconds = u64::from(*minutes) * 60;

                last_lock.as_u64().saturating_add(seconds) < now.as_u64()
            }
            Self::Never => false,
        }
    }
}

impl From<u8> for ProtectionAutoLock {
    fn from(value: u8) -> Self {
        if value > 0 && value < 255 {
            Self::Minutes(value)
        } else if value == 255 {
            Self::Never
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
            ProtectionAutoLock::Never => 255,
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

/// Struct keeping track of Pin authentication attempts
///
#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("pin_protection")]
pub struct PinProtection {
    /// There is only one entry of `PinProtection`
    /// stored in database.
    ///
    #[IdField]
    pub local_id: SingleEntryId,

    /// How many unsuccessful attempts where made to authenticate
    #[DbField]
    pub attempts: u8,

    /// When last attempt was made
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
    /// Create new `PinProtection` model.
    ///
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

    /// Return remaining attempts to verify PIN code.
    ///
    /// The reason behaind returning always 1 when attempts are greater than
    /// the allowed number of attempts is that when you would have gone done to zero
    /// your database is already cleared.
    ///
    /// So in theory there is no real life scenarion in which the number returned should be
    /// lower than 1. There is also no real life reason to force the number one as the min
    /// value BUT it has benefits when it would come to reducing number of allowed attempts.
    ///
    #[must_use]
    pub fn remaining_attempts(&self) -> u32 {
        u32::from(PinCode::MAX_ATTEMPTS.saturating_sub(self.attempts).max(1))
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

/// An error during SQL deserialization.
/// It means we expected [`MAGIC_ID`] but got {0}
#[derive(Debug, thiserror::Error)]
#[error("Expected constant {expected} local id but got {got}")]
struct NotAMagicLocalIdError {
    expected: u32,
    got: u32,
}

/// `SingleEntry` local id. This is a special value that ALWAYS must be equal the constant
/// This local id type is shared between `AppSettings` & `PinProtection` to make sure there is
/// only one entry stored. [`MAGIC_ID`]
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
    use crate::{db::migrations::migrate_account_db, tests::common::new_core_test_connection};
    use test_case::test_case;

    #[test_case(0, ProtectionAutoLock::Always)]
    #[test_case(1, ProtectionAutoLock::Minutes(1))]
    #[test_case(60, ProtectionAutoLock::Minutes(60))]
    #[test_case(255, ProtectionAutoLock::Never)]
    fn test_from_u8_for_protection_auto_lock(val: u8, expected: ProtectionAutoLock) {
        assert_eq!(ProtectionAutoLock::from(val), expected);
    }

    #[test_case(ProtectionAutoLock::Always, 0)]
    #[test_case(ProtectionAutoLock::Minutes(1), 1)]
    #[test_case(ProtectionAutoLock::Minutes(60), 60)]
    #[test_case(ProtectionAutoLock::Minutes(255), 255)]
    #[test_case(ProtectionAutoLock::Never, 255)]
    fn test_from_protection_auto_lock_for_u8(val: ProtectionAutoLock, expected: u8) {
        assert_eq!(u8::from(val), expected);
    }

    #[test_case(0 => 10; "TEST0: No attempts have been made")]
    #[test_case(1 => 9; "TEST1: One attempt has been made")]
    #[test_case(9 => 1; "TEST2: Nine attempts have been made")]
    #[test_case(10 => 1; "TEST3: Ten attempts have been made - Equal to allowed")]
    #[test_case(11 => 1; "TEST4: Eleven attempts have been made - More than allowed")]
    fn remaining_attempts(attempts: u8) -> u32 {
        let pinpro = PinProtection {
            local_id: SingleEntryId,
            attempts,
            last_access_unixepoch: 0,
            row_id: None,
        };

        pinpro.remaining_attempts()
    }

    const ONE_HOUR: u64 = 3600;
    const ONE_MINUTE: u64 = 60;
    const TWO_MINUTES: u64 = 120;

    #[test_case(ProtectionAutoLock::Always, 0, 0 => true; "TEST 0 AutoLock::Always returns true")]
    #[test_case(ProtectionAutoLock::Always, ONE_HOUR, 0 => true; "TEST 1 AutoLock::Always returns true")]
    #[test_case(ProtectionAutoLock::Always, 0, ONE_HOUR => true; "TEST 2 AutoLock::Always returns true")]
    #[test_case(ProtectionAutoLock::Minutes(1), ONE_MINUTE, 0 => false; "TEST 3 When minutes passed are equal to allowed")]
    #[test_case(ProtectionAutoLock::Minutes(1), ONE_MINUTE + 1, 0 => true; "TEST 4 When minutes passed from lock are more than allowed")]
    #[test_case(ProtectionAutoLock::Minutes(1), TWO_MINUTES + 1, ONE_MINUTE => true; "TEST 5 When minutes passed from lock are more than allowed but last lock is not 0")]
    #[test_case(ProtectionAutoLock::Minutes(60), ONE_HOUR, 0 => false; "TEST 6 When 60 minutes equal")]
    #[test_case(ProtectionAutoLock::Minutes(60), ONE_HOUR + 1, 0 => true; "TEST 6 When 60 minutes passed")]
    #[test_case(ProtectionAutoLock::Never, 0, 0 => false; "TEST 7 AutoLock::Never returns false")]
    #[test_case(ProtectionAutoLock::Never, ONE_HOUR, 0 => false; "TEST 8 AutoLock::Never returns false")]
    #[test_case(ProtectionAutoLock::Never, 0, ONE_HOUR => false; "TEST 9 AutoLock::Never returns false")]
    fn should_autolock(autolock: ProtectionAutoLock, now: u64, last_lock: u64) -> bool {
        autolock.should_autolock(UnixTimestamp::new(now), UnixTimestamp::new(last_lock))
    }

    #[tokio::test]
    async fn app_settings_autolock() {
        let stash = new_core_test_connection().await;
        migrate_account_db(&stash).await.unwrap();
        let mut tether = stash.connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;

        app_settings.set_biometrics();
        app_settings.auto_lock = ProtectionAutoLock::Minutes(10);

        tether
            .tx(async |tx| {
                app_settings.save(tx).await?;
                Result::<(), StashError>::Ok(())
            })
            .await
            .unwrap();

        // Last lock defaults no longer defaults to 0, so it will return `false`
        assert!(!app_settings.should_auto_lock(&mut tether).await.unwrap());
        let last_lock_1 = app_settings.lock_accessed_unixepoch;
        // Last lock was updated in last call, it will return `false`
        assert!(!app_settings.should_auto_lock(&mut tether).await.unwrap());
        // and any subsequent call for next 10 minutes will also return `false`
        assert!(!app_settings.should_auto_lock(&mut tether).await.unwrap());
        let last_lock_2 = app_settings.lock_accessed_unixepoch;

        assert_eq!(last_lock_1, last_lock_2);

        // After 10 minutes, it will return `true`
        // We need to subtract 10 minutes from the last lock time
        // as we cannot move time backwards
        app_settings.lock_accessed_unixepoch = last_lock_2.saturating_sub(10 * 60 + 1);

        tether
            .tx(async |tx| {
                app_settings.save(tx).await?;
                Result::<(), StashError>::Ok(())
            })
            .await
            .unwrap();

        // After 10 minutes, it will return `true`
        assert!(app_settings.should_auto_lock(&mut tether).await.unwrap());
    }
}
