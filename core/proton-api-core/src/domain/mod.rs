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
