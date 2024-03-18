//! Proton key domain types.
mod keys;
use std::fmt::{Display, Formatter};

pub use keys::*;
mod address_keys;
pub use address_keys::*;
mod public_address_keys;
pub use public_address_keys::*;
mod user_keys;
use serde_repr::{Deserialize_repr, Serialize_repr};
pub use user_keys::*;
mod signed_key_list;
pub use signed_key_list::*;
mod organization_keys;
pub use organization_keys::*;

use crate::errors::KeyError;

/// Represents a key unlock result.
///
/// Contains all unlocked keys and errors for unlock attempts
/// that have failed.
pub struct UnlockResult<T> {
    /// The unlocked keys.
    pub unlocked_keys: Vec<T>,
    /// Keys that have failed to unlock.
    pub failed: Vec<KeyError>,
}

impl<T> From<UnlockResult<T>> for Vec<T> {
    fn from(value: UnlockResult<T>) -> Self {
        value.unlocked_keys
    }
}

#[derive(Debug, Copy, Clone, Serialize_repr, Deserialize_repr, Eq, PartialEq, Hash)]
#[repr(u8)]
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
