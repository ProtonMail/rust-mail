use chrono::Utc;
use proton_crypto_pin_hash::bcrypt::{HashingError, ProtonHash, hash, verify};
use secrecy::{ExposeSecret, SecretString};
use stash::stash::StashError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::models::{AppProtection, AppSettings, PinProtection};
use crate::os::StoreInKeyChain;
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
    ValidationFailed,
    #[error("Could not encrypt the PIN, details: `{0}`")]
    HashError(#[from] HashingError),
    #[error("Error while interacting with keychain, details: `{0}`")]
    Keychain(#[from] CoreContextError),
    #[error("Could not store data in database, details: `{0}`")]
    StashError(#[from] StashError),
    #[error("Could not join future, details: `{0}`")]
    JoinError(#[from] JoinError),
}

/// Struct to group PIN code functionality
///
pub struct PinCode;

impl PinCode {
    const MAX_ATTEMPTS: u8 = 10;
    const MIN_PASSWD_LEN: usize = 4;
    const MAX_PASSWD_LEN: usize = 21;
    const HIGHEST_SINGLE_DIGIT: u8 = 9;

    /// Creates new PIN
    ///
    /// Stores `PinProtection` in account database and PIN hash in keychain
    ///
    /// Method does not verify old PIN if existed it is up to client to make that
    /// verification.
    ///
    pub async fn create_pin<P: AsRef<[u8]>>(ctx: &Context, pin: P) -> Result<(), PinError> {
        let pin = pin.as_ref().to_vec();
        let pin_len = pin.len();

        if pin_len < Self::MIN_PASSWD_LEN {
            return Err(PinError::TooShort);
        }

        if pin_len > Self::MAX_PASSWD_LEN {
            return Err(PinError::TooLong);
        }

        if pin.iter().any(|num| *num > Self::HIGHEST_SINGLE_DIGIT) {
            return Err(PinError::Malformed);
        }

        // We have no guarantees that hashing function will not block whole runtime
        // Better be safe than sorry.
        let secret = tokio::task::spawn_blocking(move || hash(pin).map(PinHash)).await??;

        ctx.store_secret(secret)?;

        let mut this = PinProtection::new();
        let mut tether = ctx.account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;

        app_settings.protection = AppProtection::Pin;

        let bond = tether.transaction().await?;
        this.save(&bond).await?;
        app_settings.save(&bond).await?;
        bond.commit().await?;

        Ok(())
    }

    /// Validate PIN value
    ///
    /// This method will be utilized to verify user if he is eligible person to access the app.
    ///
    pub async fn validate_pin<P: AsRef<[u8]>>(ctx: &Context, pin: P) -> Result<(), PinError> {
        let mut tether = ctx.account_stash().connection();
        let app_settings = AppSettings::get_or_default(&tether).await;

        if matches!(app_settings.protection, AppProtection::Pin) {
            let Some(mut pin_protection) = PinProtection::get(&tether).await? else {
                return Err(PinError::MissingPinMetadata);
            };

            if pin_protection.attempts >= Self::MAX_ATTEMPTS {
                tracing::error!("Nuking databases");
                return Err(PinError::TooManyAttempts);
            }

            let now = Utc::now().timestamp();

            if pin_protection.last_access_unixepoch == now {
                return Err(PinError::TooFrequentAttempts);
            }

            let Some(secret) = ctx.load_secret::<PinHash>()? else {
                return Err(PinError::MissingPinHash);
            };

            let bond = tether.transaction().await?;
            pin_protection.last_access_unixepoch = now;

            // We have no guarantees that hashing function will not block whole runtime
            // Better be safe than sorry.
            let pin = pin.as_ref().to_vec();
            let success = tokio::task::spawn_blocking(move || verify(pin, &secret.0)).await??;

            if success {
                pin_protection.attempts = 0;
                pin_protection.save(&bond).await?;
                bond.commit().await?;

                Ok(())
            } else {
                pin_protection.attempts += 1;
                pin_protection.save(&bond).await?;
                bond.commit().await?;

                Err(PinError::ValidationFailed)
            }
        } else {
            Ok(())
        }
    }
}

struct PinHash(ProtonHash);

impl StoreInKeyChain for PinHash {
    fn kind() -> crate::os::KeyChainEntryKind {
        crate::os::KeyChainEntryKind::PinHash
    }
    fn from_stored_string(
        s: SecretString,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        s.expose_secret()
            .parse()
            .map(PinHash)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
    }

    fn to_stored_string(&self) -> SecretString {
        SecretString::new(self.0.to_string())
    }
}
