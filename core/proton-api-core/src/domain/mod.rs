//! Domain Types.

mod event;
mod human_verification;
mod user;
mod user_settings;

pub use event::*;
pub use human_verification::*;
pub use user::*;
pub use user_settings::*;

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

new_integer_enum!(u8, ProtonBoolean {
    False = 0,
    True = 1,
});

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
