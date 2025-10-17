use std::time::Duration;

use derive_more::derive::TryFrom;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Transaction, Value,
    ValueRef,
};
use stash::macros::Model;
use stash::orm::{Model, ModelHooks};
use stash::stash::{StashError, Tether};
use tracing::{debug, instrument};

use crate::Context;
use crate::pin_code::PinCode;
use smart_default::SmartDefault;

/// Struct Representing `AppSettings` - cross accounts settings of the application.
///
/// This model is stored in account (shared) database.
///
#[derive(Debug, Clone, PartialEq, Model, SmartDefault)]
#[ModelHooks]
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
    #[default = true]
    pub use_alternative_routing: bool,
}

impl ModelHooks for AppSettings {
    fn before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        // Make sure there will be only one row.
        if Self::load_by_id_sync(SingleEntryId, tx)?.is_some() {
            self.local_id = SingleEntryId;
        }
        Ok(())
    }
}

impl AppSettings {
    pub fn set_biometrics(&mut self) {
        if let AppProtection::None = self.protection {
            self.protection = AppProtection::Biometrics;
        }
    }

    pub fn unset_biometrics(&mut self) {
        if let AppProtection::Biometrics = self.protection {
            self.protection = AppProtection::None;
        }
    }

    #[instrument(skip_all)]
    pub fn should_auto_lock(&self, ctx: &Context) -> bool {
        debug!(protection=?self.protection, "Checking auto-lock");

        if self.protection.is_unset() {
            false
        } else {
            let lock_elapsed = ctx.clock().auto_lock_elapsed();
            let should_lock = self.auto_lock.should_autolock(lock_elapsed);

            debug!(?should_lock);

            // If the app is not supposed to lock, we need to mark that the auto lock has been accessed
            // so that the timer is reset. So that the next time the app is opened, it will not lock.
            if !should_lock {
                ctx.clock().auto_lock_accessed();
            }

            should_lock
        }
    }

    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(SingleEntryId, tether).await
    }

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
    pub(crate) fn should_autolock(self, locked_for: Option<Duration>) -> bool {
        match self {
            Self::Always => true,

            Self::Minutes(minutes) => match locked_for {
                Some(locked_for) => locked_for.as_secs() > u64::from(minutes) * 60,
                None => true,
            },

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
#[ModelHooks]
#[TableName("pin_protection")]
pub struct PinProtection {
    #[IdField]
    pub local_id: SingleEntryId,

    #[DbField]
    pub attempts: u8,
}

impl ModelHooks for PinProtection {
    fn before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        if Self::load_by_id_sync(SingleEntryId, tx)?.is_some() {
            self.local_id = SingleEntryId;
        }
        Ok(())
    }
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
        }
    }

    /// Get the pin protection from database
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(SingleEntryId, tether).await
    }

    /// Return remaining attempts to verify PIN code.
    ///
    #[must_use]
    pub fn remaining_attempts(&self) -> u32 {
        u32::from(PinCode::MAX_ATTEMPTS.saturating_sub(self.attempts))
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Default)]
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
    use std::time::Duration;

    use crate::test_utils::test_context::TestContext;

    use super::*;
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

    #[test_case(0 => 10; "TEST0: 1st attempt is allowed")]
    #[test_case(1 => 9; "TEST1: 2nd attempt is allowed")]
    #[test_case(2 => 8; "TEST2: 3rd attempt is allowed")]
    #[test_case(3 => 7; "TEST3: 4th attempt is allowed")]
    #[test_case(4 => 6; "TEST4: 5th attempt is allowed")]
    #[test_case(5 => 5; "TEST5: 6th attempt is allowed")]
    #[test_case(6 => 4; "TEST6: 7th attempt is allowed")]
    #[test_case(7 => 3; "TEST7: 8th attempt is allowed")]
    #[test_case(8 => 2; "TEST8: 9th attempt is allowed")]
    #[test_case(9 => 1; "TEST9: 10th attempt is allowed")]
    #[test_case(10 => 0; "TEST10: 11th attempt is not allowed")]
    #[test_case(11 => 0; "TEST11: 12th attempt is not allowed")]
    fn remaining_attempts(attempts: u8) -> u32 {
        let pinpro = PinProtection {
            local_id: SingleEntryId,
            attempts,
        };

        pinpro.remaining_attempts()
    }

    const ONE_MINUTE: u64 = 60;
    const ONE_HOUR: u64 = 60 * ONE_MINUTE;

    #[test_case(ProtectionAutoLock::Always, None => true)]
    #[test_case(ProtectionAutoLock::Always, Some(0) => true)]
    #[test_case(ProtectionAutoLock::Always, Some(ONE_HOUR) => true)]
    // --
    #[test_case(ProtectionAutoLock::Minutes(1), None => true)]
    #[test_case(ProtectionAutoLock::Minutes(1), Some(1) => false)]
    #[test_case(ProtectionAutoLock::Minutes(1), Some(ONE_MINUTE) => false)]
    #[test_case(ProtectionAutoLock::Minutes(1), Some(ONE_MINUTE + 1) => true)]
    #[test_case(ProtectionAutoLock::Minutes(60), Some(ONE_HOUR) => false)]
    #[test_case(ProtectionAutoLock::Minutes(60), Some(ONE_HOUR + 1) => true)]
    // --
    #[test_case(ProtectionAutoLock::Never, None => false)]
    #[test_case(ProtectionAutoLock::Never, Some(0) => false)]
    #[test_case(ProtectionAutoLock::Never, Some(ONE_HOUR) => false)]
    fn should_autolock(autolock: ProtectionAutoLock, locked_for_s: Option<u64>) -> bool {
        let locked_for = locked_for_s.map(Duration::from_secs);

        autolock.should_autolock(locked_for)
    }

    #[tokio::test]
    async fn app_settings_autolock() {
        let test_ctx = TestContext::new().await;
        let core_ctx = test_ctx.core_context();
        let tether = core_ctx.account_stash().connection().await.unwrap();
        let mut app_settings = AppSettings::get_or_default(&tether).await;

        app_settings.set_biometrics();
        app_settings.auto_lock = ProtectionAutoLock::Minutes(10);

        // First calls to should_auto_lock will return true
        assert!(app_settings.should_auto_lock(core_ctx));
        assert!(app_settings.should_auto_lock(core_ctx));

        // Ticking the clock will not change the result
        core_ctx.clock().auto_lock_tick();
        assert!(app_settings.should_auto_lock(core_ctx));

        // We need to mark that the auto lock has been accessed
        // in order to reset the timer
        core_ctx.clock().auto_lock_accessed();
        core_ctx.clock().auto_lock_tick();

        // Now the app is unlocked for the next 10 minutes
        let last_lock_1 = core_ctx.clock().auto_lock_elapsed();
        assert!(!app_settings.should_auto_lock(core_ctx));
        assert!(!app_settings.should_auto_lock(core_ctx));
        let last_lock_2 = core_ctx.clock().auto_lock_elapsed();

        assert!(last_lock_1 < last_lock_2);

        core_ctx.clock().auto_lock_reset();

        // After reset, it will return `true`
        assert!(app_settings.should_auto_lock(core_ctx));

        // Till the auto lock is not accessed it will return `true`
        assert!(app_settings.should_auto_lock(core_ctx));

        // Now it will return `false` as we have accessed the app
        core_ctx.clock().auto_lock_tick();
        assert!(!app_settings.should_auto_lock(core_ctx));
    }
}
