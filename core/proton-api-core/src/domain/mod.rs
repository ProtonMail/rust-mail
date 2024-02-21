//! Domain Types.

mod event;
mod human_verification;
mod user;

pub use event::*;
pub use human_verification::*;
pub use user::*;

use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::{Display, Formatter};

pub type SecretString = secrecy::SecretString;
pub use secrecy::ExposeSecret;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
/// Types of Two Factor Authentication.
pub enum TwoFactorAuth {
    None,
    TOTP,
    FIDO2,
}

impl Display for TwoFactorAuth {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TwoFactorAuth::None => "None".fmt(f),
            TwoFactorAuth::TOTP => "TOTP".fmt(f),
            TwoFactorAuth::FIDO2 => "FIDO2".fmt(f),
        }
    }
}

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProtonBoolean {
    False = 0,
    True = 1,
}

impl Default for ProtonBoolean {
    fn default() -> Self {
        Self::False
    }
}

impl From<ProtonBoolean> for bool {
    fn from(value: ProtonBoolean) -> Self {
        value == ProtonBoolean::True
    }
}

impl From<bool> for ProtonBoolean {
    fn from(v: bool) -> Self {
        if v {
            ProtonBoolean::True
        } else {
            ProtonBoolean::False
        }
    }
}

impl Display for ProtonBoolean {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtonBoolean::False => {
                write!(f, "0")
            }
            ProtonBoolean::True => {
                write!(f, "1")
            }
        }
    }
}

#[cfg(feature = "sql")]
impl proton_sqlite3::rusqlite::types::ToSql for ProtonBoolean {
    fn to_sql(
        &self,
    ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>> {
        Ok(proton_sqlite3::rusqlite::types::ToSqlOutput::Owned(
            proton_sqlite3::rusqlite::types::Value::Integer(*self as i64),
        ))
    }
}

#[cfg(feature = "sql")]
impl proton_sqlite3::rusqlite::types::FromSql for ProtonBoolean {
    fn column_result(
        value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
    ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(ProtonBoolean::False),
            1 => Ok(ProtonBoolean::True),
            v => Err(proton_sqlite3::rusqlite::types::FromSqlError::OutOfRange(v)),
        }
    }
}
