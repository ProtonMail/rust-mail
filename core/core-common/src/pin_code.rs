use std::io::Error as IoError;
use std::ops::Deref;
use std::sync::Arc;

use chrono::Utc;
use proton_crypto_pin_hash::argon2::{Argon2HashingError, ProtonArgon2Hash};
use secrecy::{ExposeSecret, SecretString};
use stash::stash::StashError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::models::{AppProtection, AppSettings, ModelExtension, PinProtection};
use crate::os::{KeyChainError, StoreInKeyChain};
use crate::{Context, CoreContextError};

#[derive(Debug, Error)]
pub enum PinError {
    #[error("Provided pin is too short")]
    TooShort,
    #[error("Provided pin is too long")]
    TooLong,
    #[error("Provided pin should contain single digit numbers only")]
    Malformed,
    #[error("Database in incorrect state, cannot validate PIN")]
    MissingPinMetadata,
    #[error("There is no PIN Hash in keychain, cannot validate PIN")]
    MissingPinHash,
    #[error("Too many attempts")]
    TooManyAttempts,
    #[error("Too frequent attempts, attempts can be made at least one second appart")]
    TooFrequentAttempts,
    #[error("Incorrect PIN")]
    IncorrectPin,
    #[error("Could not encrypt the PIN, details: `{0}`")]
    HashError(#[from] Argon2HashingError),
    #[error("Error while interacting with keychain, details: `{0}`")]
    Keychain(#[from] KeyChainError),
    #[error("Could not store data in database, details: `{0}`")]
    StashError(#[from] StashError),
    #[error("Could not join future, details: `{0}`")]
    JoinError(#[from] JoinError),
    #[error("Core context error, details: `{0}`")]
    CoreContext(#[from] CoreContextError),
    #[error("IO Error, details: `{0}`")]
    IoError(#[from] IoError),
}

/// Struct to group PIN code functionality
///
pub struct PinCode;

impl PinCode {
    pub const MAX_ATTEMPTS: u8 = 10;
    const MIN_PASSWD_LEN: usize = 4;
    const MAX_PASSWD_LEN: usize = 21;
    const HIGHEST_SINGLE_DIGIT: u32 = 9;

    /// Creates new PIN
    ///
    /// Stores `PinProtection` in account database and PIN hash in keychain
    ///
    /// Method does not verify old PIN if existed it is up to client to make that
    /// verification.
    ///
    pub async fn create_pin(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        let pin_len = pin.len();

        if pin_len < Self::MIN_PASSWD_LEN {
            return Err(PinError::TooShort);
        }

        if pin_len > Self::MAX_PASSWD_LEN {
            return Err(PinError::TooLong);
        }

        let pin = Self::sanitize_pin(pin)?;

        // We have no guarantees that hashing function will not block whole runtime
        // Better be safe than sorry.
        let ctx_clone = ctx.clone();
        tokio::task::spawn_blocking(move || {
            let secret = ProtonArgon2Hash::hash(pin).map(PinHash)?;
            ctx_clone.store_secret(secret)?;

            Result::<(), PinError>::Ok(())
        })
        .await??;

        let mut this = PinProtection::new();
        let mut tether = ctx.account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;

        app_settings.protection = AppProtection::Pin;

        tether
            .tx(async |bond| -> Result<(), StashError> {
                this.save(bond).await?;
                app_settings.save(bond).await?;

                Ok(())
            })
            .await?;

        Ok(())
    }

    /// Validate PIN value
    ///
    /// This method will be utilized to verify user if he is eligible person to access the app.
    ///
    pub async fn validate_pin(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        let pin = Self::sanitize_pin(pin)?;
        let mut tether = ctx.account_stash().connection();
        let app_settings = AppSettings::get_or_default(&tether).await;

        if matches!(app_settings.protection, AppProtection::Pin) {
            let Some(mut pin_protection) = PinProtection::get(&tether).await? else {
                return Err(PinError::MissingPinMetadata);
            };

            let now = Utc::now().timestamp();

            if pin_protection.last_access_unixepoch == now {
                return Err(PinError::TooFrequentAttempts);
            }

            // We have no guarantees that hashing function will not block whole runtime
            // Better be safe than sorry.
            let ctx_clone = ctx.clone();
            let success = tokio::task::spawn_blocking(move || {
                let Some(secret) = ctx_clone.load_secret::<PinHash>()? else {
                    return Err(PinError::MissingPinHash);
                };

                Ok(secret.verify(pin)?)
            })
            .await??;

            tether
                .tx(async |bond| -> Result<(), StashError> {
                    pin_protection.last_access_unixepoch = now;

                    if success {
                        pin_protection.attempts = 0;
                        pin_protection.save(bond).await?;
                    } else {
                        pin_protection.attempts += 1;
                        pin_protection.save(bond).await?;
                    }

                    Ok(())
                })
                .await?;

            if success {
                Ok(())
            } else if pin_protection.attempts >= Self::MAX_ATTEMPTS {
                tracing::error!("All attemps to validate PIN have been used");

                Err(PinError::TooManyAttempts)
            } else {
                Err(PinError::IncorrectPin)
            }
        } else {
            Ok(())
        }
    }

    /// Delete PIN
    ///
    /// This method validates correctness of the PIN code so it proceed when presented with proper value.
    ///
    /// Chosen order of the removal is to minimalize possibility of ending up in incorrect state
    /// Firstly the database is updated and when successful the `PinHash` is removed from the `KeyChain`.
    ///
    pub async fn delete_pin(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        Self::validate_pin(ctx.clone(), pin).await?;

        let mut tether = ctx.account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;
        let pin_protection = PinProtection::get(&tether).await?;

        app_settings.protection = AppProtection::None;

        tether
            .tx(async |bond| -> Result<(), StashError> {
                app_settings.save(bond).await?;

                if let Some(pin_protection) = pin_protection {
                    pin_protection.delete(bond).await?;
                }

                Ok(())
            })
            .await?;

        tokio::task::spawn_blocking(move || ctx.delete_secret::<PinHash>()).await??;

        Ok(())
    }

    fn sanitize_pin(pin: Vec<u32>) -> Result<Vec<u8>, PinError> {
        pin.into_iter()
            .map(|num| {
                if num <= Self::HIGHEST_SINGLE_DIGIT {
                    Ok(u8::try_from(num).unwrap())
                } else {
                    Err(PinError::Malformed)
                }
            })
            .collect()
    }
}

pub(crate) struct PinHash(ProtonArgon2Hash);

impl Deref for PinHash {
    type Target = ProtonArgon2Hash;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl StoreInKeyChain for PinHash {
    fn kind() -> crate::os::KeyChainEntryKind {
        crate::os::KeyChainEntryKind::PinHash
    }
    fn from_stored_string(
        s: SecretString,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // unwrap safety: ProtonArgon2Hash::from_str returns `Infallible`
        Ok(s.expose_secret().parse().map(PinHash).unwrap())
    }

    fn to_stored_string(&self) -> SecretString {
        // unwrap safety: SecretString::from_str returns `Infallible`
        self.as_ref().parse().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::{PinCode, PinError};
    use test_case::test_case;

    #[test_case(vec![0], Ok(vec![0]))]
    #[test_case(vec![1], Ok(vec![1]))]
    #[test_case(vec![9], Ok(vec![9]))]
    #[test_case(vec![10], Err(PinError::Malformed))]
    fn test_standarize_pin(pin: Vec<u32>, expected: Result<Vec<u8>, PinError>) {
        let actual = PinCode::sanitize_pin(pin);
        if expected.is_err() {
            assert_eq!(
                actual.unwrap_err().to_string(),
                expected.unwrap_err().to_string()
            );
        } else {
            assert_eq!(actual.unwrap(), expected.unwrap());
        }
    }
}
