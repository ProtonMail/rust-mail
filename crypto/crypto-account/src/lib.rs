//! Provides cryptography domains and utility for Proton account.

mod constants;
use constants::{
    FLAG_EMAIL_NO_ENCRYPT, FLAG_EMAIL_NO_SIGN, FLAG_NOT_COMPROMISED, FLAG_NOT_OBSOLETE,
};
mod crypto;
pub mod domain;
pub mod errors;
pub mod salts;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

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

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
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
