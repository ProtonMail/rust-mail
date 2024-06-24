//! Provides cryptography domains and utility for Proton account.

mod constants;
use constants::{
    FLAG_EMAIL_NO_ENCRYPT, FLAG_EMAIL_NO_SIGN, FLAG_NOT_COMPROMISED, FLAG_NOT_OBSOLETE,
};
pub mod contacts;
mod crypto;
pub mod errors;
pub mod keys;
pub mod salts;

// re-export crypto crate;
pub use proton_crypto;

macro_rules! string_id {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        #[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
        $(#[$meta])*
        pub struct $name(pub String);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)

            }
        }

        impl<T: Into<String>> From<T> for $name {
            fn from(value: T) -> Self {
                Self(value.into())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

pub(crate) use string_id;
