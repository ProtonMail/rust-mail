use chrono::{DateTime, Local, MappedLocalTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stores in seconds.
#[derive(Debug, Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct UnixTimestamp(u64);

impl UnixTimestamp {
    #[must_use]
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn now() -> Self {
        Self(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before Unix epoch")
                .as_secs(),
        )
    }

    #[must_use]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn to_date_time(self) -> Option<DateTime<Local>> {
        self.to_chrono_dt(&Local)
    }

    #[must_use]
    pub fn to_date_time_utc(self) -> Option<DateTime<Utc>> {
        self.to_chrono_dt(&Utc)
    }

    #[allow(clippy::cast_possible_wrap)] // manually checked
    fn to_chrono_dt<Tz: TimeZone>(self, tz: &Tz) -> Option<DateTime<Tz>> {
        if self.0 >= i64::MAX as u64 {
            return None;
        }
        //Note: ambiguous is never returned from chrono conversion
        match tz.timestamp_opt(self.0 as i64, 0) {
            MappedLocalTime::Single(v) => Some(v),
            MappedLocalTime::None => None,
            MappedLocalTime::Ambiguous(_, _) => unreachable!(),
        }
    }

    #[must_use]
    pub fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }

    #[must_use]
    pub fn saturating_sub(self, rhs: u64) -> Self {
        Self(self.0.saturating_sub(rhs))
    }
}

impl std::fmt::Display for UnixTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for UnixTimestamp {
    fn default() -> Self {
        Self::now()
    }
}

impl<Tz: TimeZone> From<DateTime<Tz>> for UnixTimestamp {
    fn from(value: DateTime<Tz>) -> Self {
        Self(value.timestamp().unsigned_abs())
    }
}

impl From<u64> for UnixTimestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<&jiff::Zoned> for UnixTimestamp {
    fn from(dt: &jiff::Zoned) -> Self {
        #[allow(
            clippy::cast_sign_loss,
            reason = "jiff::Zoned's timestamp is always positive"
        )]
        Self(dt.timestamp().as_second() as u64)
    }
}

impl stash::exports::ToSql for UnixTimestamp {
    fn to_sql(&self) -> Result<stash::exports::ToSqlOutput<'_>, stash::exports::SqliteError> {
        self.0.to_sql()
    }
}

impl stash::exports::FromSql for UnixTimestamp {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        u64::column_result(value).map(Self)
    }
}
