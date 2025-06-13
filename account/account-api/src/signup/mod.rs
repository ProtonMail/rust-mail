use crate::countries::{COUNTRIES, Country};
use crate::prelude::{Address, Behavior, User};
use crate::shared::crypto::SharedCryptoError;
use crate::signup::state::{Recovery, StateKind, Username};
use crate::{AccountApi, ApiError};
use proton_core_api::store::{DynStore, StoreError};
use proton_core_common::device::DeviceInfo;
use proton_crypto_account::errors::{AccountCryptoError, SKLError};
use proton_crypto_account::{proton_crypto::CryptoError, salts::SaltError};
use state::State;
use std::fmt::Debug;
use thiserror::Error;

pub mod state;

/// Errors that can occur during the signup flow.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum SignupError {
    /// An underlying API call failed.
    Api(#[from] ApiError),

    /// A crypto error occurred.
    Crypto(#[from] SignupCryptoError),

    /// Signup is blocked (e.g., anti-abuse).
    #[error("Signup blocked by server")]
    SignupBlockedByServer,

    /// Username is unavailable.
    #[error("Username unavailable")]
    UsernameUnavailable,

    /// Placeholder for errors during account creation step.
    #[error("Account creation failed")]
    AccountCreationFailed,

    /// Placeholder for errors during address setup step.
    #[error("Address setup failed")]
    AddressSetupFailed,

    /// Placeholder for errors during key setup step.
    #[error("Key setup failed")]
    KeySetupFailed,

    /// The auth info could not be set in the store.
    #[error("SetAuthInfo failed: {0}")]
    SetAuthInfoFailed(StoreError),

    /// The user data could not be set in the store.
    #[error("SetUserData failed: {0}")]
    SetUserDataFailed(StoreError),

    /// The requested operation is not valid in the current state of the flow.
    #[error("Operation is not valid in the current state")]
    InvalidState,

    /// The recovery email format is invalid
    #[error("Recovery email format is invalid")]
    RecoveryEmailInvalid,

    /// The recovery phone number format is invalid
    #[error("Recovery phone number format is invalid")]
    RecoveryPhoneNumberInvalid,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct SignupCryptoError(String);

impl From<SaltError> for SignupError {
    fn from(e: SaltError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<CryptoError> for SignupError {
    fn from(e: CryptoError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<AccountCryptoError> for SignupError {
    fn from(e: AccountCryptoError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<SKLError> for SignupError {
    fn from(e: SKLError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<SharedCryptoError> for SignupError {
    fn from(e: SharedCryptoError) -> Self {
        match e {
            SharedCryptoError::Salt(e) => e.into(),
            SharedCryptoError::Crypto(e) => e.into(),
            SharedCryptoError::AccountCrypto(e) => e.into(),
            SharedCryptoError::SKL(e) => e.into(),
        }
    }
}

/// Info needed to construct the challenge payload.
#[derive(Debug, Clone)]
pub struct ChallengeInfo {
    /// Client version to be used for a challenge (e.g. `mail-ios-v4`).
    pub product_version: String,
    /// Device fingerprint.
    pub device_info: Option<DeviceInfo>,
    /// User behaviour while entering the recovery method (if applicable).
    pub recovery_behavior: Option<Behavior>,
    /// User behaviour while entering the username (if applicable).
    pub username_behavior: Option<Behavior>,
}

/// A signup flow that can be used to sign up a user.
///
/// The flow guides the user through the signup process, ensuring all necessary steps
/// are completed in the correct order.
pub struct SignupFlow {
    store: DynStore,
    state: Vec<State>,
    domains: Vec<String>,
    countries: Vec<Country>,
}

impl SignupFlow {
    /// Create a new signup flow, implicitly fetching available domains.
    pub async fn new(
        client: muon::Client,
        store: DynStore,
        challenge_info: ChallengeInfo,
    ) -> Result<Self, SignupError> {
        let domains = client.get_available_domains(None).await?.domains;
        let countries = COUNTRIES.to_owned();
        let state = vec![State::new(client, challenge_info)];

        Ok(Self {
            store,
            state,
            domains,
            countries,
        })
    }

    /// Get available domains.
    #[must_use]
    pub fn available_domains(&self) -> &[String] {
        &self.domains
    }

    /// Get available countries.
    #[must_use]
    pub fn available_countries(&self) -> &[Country] {
        &self.countries
    }

    /// Get the kind of the current state.
    pub fn kind(&self) -> Result<StateKind, SignupError> {
        Ok(self.state()?.kind())
    }

    /// Submit Proton Internal Username
    pub async fn submit_internal_username(
        &mut self,
        username: String,
        domain: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let username = Username::Internal { username, domain };

        let next = self.state()?.submit_username(username, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit Proton External email
    pub async fn submit_external_username(&mut self, email: String) -> Result<(), SignupError> {
        let username = Username::External { email };

        let next = self.state()?.submit_username(username, None).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit password.
    pub fn submit_password(&mut self, password: String) -> Result<(), SignupError> {
        let next = self.state()?.submit_password(password)?;

        self.state.push(next);

        Ok(())
    }

    /// Submit a recovery email.
    pub async fn submit_recovery_email(
        &mut self,
        email: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let recovery = Recovery::Email(email);

        let next = self.state()?.submit_recovery(recovery, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit a recovery phone number.
    pub async fn submit_recovery_phone(
        &mut self,
        phone: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let recovery = Recovery::Phone(phone);

        let next = self.state()?.submit_recovery(recovery, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Skip recovery information.
    pub async fn skip_recovery(&mut self) -> Result<(), SignupError> {
        let recovery = Recovery::None;

        let next = self.state()?.submit_recovery(recovery, None).await?;

        self.state.push(next);

        Ok(())
    }

    /// Create the account.
    pub async fn create(&mut self) -> Result<(), SignupError> {
        let store = DynStore::clone(&self.store);

        let next = self.state()?.create(store).await?;

        self.state.push(next);

        Ok(())
    }

    /// Complete the signup flow.
    pub fn complete(&mut self) -> Result<(muon::Client, User, Address), SignupError> {
        self.state()?.complete()
    }

    /// Return to the previous state.
    pub fn back(&mut self) -> Result<(), SignupError> {
        if self.state.len() < 2 {
            return Err(SignupError::InvalidState);
        }

        self.state.pop();

        Ok(())
    }

    fn state(&self) -> Result<State, SignupError> {
        self.state.last().cloned().ok_or(SignupError::InvalidState)
    }
}
