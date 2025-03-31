use std::io::Error as IoError;
use std::ops::Deref;
use std::sync::Arc;

use chrono::Utc;
use futures::future::try_join_all;
use proton_crypto_pin_hash::argon2::{Argon2HashingError, ProtonArgon2Hash};
use secrecy::{ExposeSecret, SecretString};
use stash::stash::{StashError, Tether};
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
    pub const MAX_ATTEMPTS: u8 = 9; // Counting from 0 -> 10 attmepts
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
    pub async fn create_pin<P: AsRef<[u8]>>(ctx: Arc<Context>, pin: P) -> Result<(), PinError> {
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
    pub async fn validate_pin<P: AsRef<[u8]>>(ctx: Arc<Context>, pin: P) -> Result<(), PinError> {
        let mut tether = ctx.account_stash().connection();
        let app_settings = AppSettings::get_or_default(&tether).await;

        if matches!(app_settings.protection, AppProtection::Pin) {
            let Some(mut pin_protection) = PinProtection::get(&tether).await? else {
                return Err(PinError::MissingPinMetadata);
            };

            if pin_protection.attempts >= Self::MAX_ATTEMPTS {
                tracing::error!(
                    "All attemps to validate PIN have been used, nuking application data"
                );
                nuke_application_data(ctx).await?;

                return Err(PinError::TooManyAttempts);
            }

            let now = Utc::now().timestamp();

            if pin_protection.last_access_unixepoch == now {
                return Err(PinError::TooFrequentAttempts);
            }

            // We have no guarantees that hashing function will not block whole runtime
            // Better be safe than sorry.
            let pin = pin.as_ref().to_vec();
            let success = tokio::task::spawn_blocking(move || {
                let Some(secret) = ctx.load_secret::<PinHash>()? else {
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
    pub async fn delete_pin<P: AsRef<[u8]>>(ctx: Arc<Context>, pin: P) -> Result<(), PinError> {
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
}

struct PinHash(ProtonArgon2Hash);

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

const QUERY_LIST_TABLES: &str = "SELECT name as value FROM sqlite_master WHERE type='table'";

async fn nuke_application_data(ctx: Arc<Context>) -> Result<(), PinError> {
    tracing::error!("Fetching all logged in users.");
    let all_user_ctxs = ctx.get_all_logged_in_user_ctx().await?;
    let users = ctx.get_accounts().await?;

    tracing::error!("Logout and delete all accounts");
    for user in users {
        ctx.logout_account(user.remote_id.clone()).await?;
        ctx.delete_account(user.remote_id).await?;
    }

    tracing::error!("Removing all user data");
    let iter = all_user_ctxs.iter().map(|ctx| async {
        let tether = ctx.stash().connection();

        drop_all_tables_in_database(tether).await?;

        Result::<(), PinError>::Ok(())
    });

    try_join_all(iter).await?;

    tracing::error!("Removing all remaining account data");
    let tether = ctx.account_stash().connection();

    drop_all_tables_in_database(tether).await?;

    tracing::error!("Removing all cached filesystem data");
    if let Err(e) = tokio::fs::remove_dir_all(ctx.get_cache_path()).await {
        tracing::error!("Could not remove cached data in filesystem, details: `{e}`");
    }

    tracing::error!("Application's data has been cleared successfuly");

    Ok(())
}

async fn drop_all_tables_in_database(mut tether: Tether) -> Result<(), StashError> {
    tether.execute("PRAGMA foreign_keys = OFF;", vec![]).await?;

    let table_names = tether
        .query_values::<_, String>(QUERY_LIST_TABLES, vec![])
        .await?;

    let tx_res = tether
        .tx(async |tx| -> Result<(), StashError> {
            for table in table_names {
                let query = format!("DROP TABLE IF EXISTS {table};");
                if let Err(e) = tx.execute(query, vec![]).await {
                    tracing::error!("Could not drop table: `{table}`, details: `{e}`");
                }
            }

            Ok(())
        })
        .await;

    tether.execute("PRAGMA foreign_keys = ON;", vec![]).await?;

    tx_res
}
