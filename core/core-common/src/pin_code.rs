use stash::orm::Model;
use std::error::Error;
use std::io::Error as IoError;
use std::ops::Deref;
use std::sync::Arc;
use tracing::{info, instrument};

use proton_crypto_pin_hash::argon2::{Argon2HashingError, ProtonArgon2Hash};
use secrecy::{ExposeSecret, SecretString};
use stash::stash::StashError;
use thiserror::Error;
use tokio::task::{self, JoinError};

use crate::models::{AppProtection, AppSettings, ModelExtension, PinProtection};
use crate::os::{KeyChainEntryKind, KeyChainError, StoreInKeyChain};
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

pub struct PinCode;

impl PinCode {
    pub const MAX_ATTEMPTS: u8 = 10;
    const MIN_PASSWD_LEN: usize = 4;
    const MAX_PASSWD_LEN: usize = 21;
    const HIGHEST_SINGLE_DIGIT: u32 = 9;
    const PIN_CODE_ACCESS_INTERVAL: u64 = 1;

    // Note that this method does not verify old PIN if existed - it is up to
    // client to make that verification.
    #[instrument(skip_all)]
    pub async fn set(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        info!("Setting pin");

        let pin_len = pin.len();

        if pin_len < Self::MIN_PASSWD_LEN {
            return Err(PinError::TooShort);
        }

        if pin_len > Self::MAX_PASSWD_LEN {
            return Err(PinError::TooLong);
        }

        let pin = Self::sanitize(pin)?;

        // We have no guarantees that hashing function will not block whole runtime
        // Better be safe than sorry.
        task::spawn_blocking({
            let ctx = ctx.clone();

            move || {
                let secret = ProtonArgon2Hash::hash(pin).map(PinHash)?;
                ctx.store_secret(secret)?;

                Result::<(), PinError>::Ok(())
            }
        })
        .await??;

        let mut this = PinProtection::new();
        let mut tether = ctx.account_stash().connection().await?;
        let mut app_settings = AppSettings::get_or_default(&tether).await;

        app_settings.protection = AppProtection::Pin;

        tether
            .tx(async |bond| -> Result<(), StashError> {
                this.save(bond).await?;
                app_settings.save(bond).await?;

                Ok(())
            })
            .await?;

        ctx.clock().pin_code_tick();

        Ok(())
    }

    #[instrument(skip_all)]
    pub async fn verify(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        info!("Verifying pin");

        let pin = Self::sanitize(pin)?;
        let mut tether = ctx.account_stash().connection().await?;
        let app_settings = AppSettings::get_or_default(&tether).await;

        if matches!(app_settings.protection, AppProtection::Pin) {
            let Some(mut pin_protection) = PinProtection::get(&tether).await? else {
                return Err(PinError::MissingPinMetadata);
            };

            if let Some(last_access) = ctx.clock().pin_code_elapsed()
                && last_access.as_secs() <= Self::PIN_CODE_ACCESS_INTERVAL
            {
                return Err(PinError::TooFrequentAttempts);
            }

            // We have no guarantees that hashing function will not block whole runtime
            // Better be safe than sorry.
            let ctx_clone = ctx.clone();

            let success = task::spawn_blocking(move || {
                let Some(secret) = ctx_clone.load_secret::<PinHash>()? else {
                    return Err(PinError::MissingPinHash);
                };

                Ok(secret.verify(pin)?)
            })
            .await??;

            tether
                .tx(async |bond| -> Result<(), StashError> {
                    ctx.clock().pin_code_tick();

                    if success {
                        ctx.clock().auto_lock_accessed();
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
            } else if pin_protection.remaining_attempts() == 0 {
                tracing::error!("All attempts to validate PIN have been used");

                Err(PinError::TooManyAttempts)
            } else {
                Err(PinError::IncorrectPin)
            }
        } else {
            Ok(())
        }
    }

    #[instrument(skip_all)]
    pub async fn delete(ctx: Arc<Context>, pin: Vec<u32>) -> Result<(), PinError> {
        info!("Deleting pin");

        Self::verify(ctx.clone(), pin).await?;
        Self::force_delete(ctx).await
    }

    #[instrument(skip_all)]
    pub(crate) async fn force_delete(ctx: Arc<Context>) -> Result<(), PinError> {
        let mut tether = ctx.account_stash().connection().await?;
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

        task::spawn_blocking(move || ctx.delete_secret::<PinHash>()).await??;

        Ok(())
    }

    #[allow(clippy::result_large_err)]
    fn sanitize(pin: Vec<u32>) -> Result<Vec<u8>, PinError> {
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
    fn kind() -> KeyChainEntryKind {
        KeyChainEntryKind::PinHash
    }

    fn from_stored_string(s: SecretString) -> Result<Self, Box<dyn Error + Send + Sync>> {
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
    fn test_sanitize_pin(pin: Vec<u32>, expected: Result<Vec<u8>, PinError>) {
        let actual = PinCode::sanitize(pin);

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
