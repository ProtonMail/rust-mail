/// Re-export the key secret type.
pub use proton_crypto_account::salts::KeySecret;

/// Re-export the muon auth type.
pub use muon::client::{Auth, PasswordMode, Tokens};

/// The user key secret to unlock user keys.
#[derive(Debug, Clone)]
pub struct UserKeySecret(pub KeySecret);

impl UserKeySecret {
    /// Exposes the internal key secret to unlock user keys.
    #[must_use]
    pub fn expose_secret(&self) -> &KeySecret {
        &self.0
    }
}

impl<T: Into<Vec<u8>>> From<T> for UserKeySecret {
    fn from(value: T) -> Self {
        Self(KeySecret::new(value.into()))
    }
}
