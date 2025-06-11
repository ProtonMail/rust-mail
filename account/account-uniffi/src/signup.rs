#![allow(clippy::match_same_arms, clippy::missing_panics_doc)]

//! Implements the sign-up flow.

use itertools::Itertools;
use proton_account_api::countries::Country as RealCountry;
use proton_account_api::requests::UserBehavior as RealUserBehavior;
use proton_account_api::signup::SignupError as RealSignupError;
use proton_account_api::signup::SignupFlow as RealSignupFlow;
use proton_account_api::signup::state::StateKind;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use uniffi_runtime::{async_runtime, uniffi_async};

/// Errors that can occur during the signup flow, exposed via `UniFFI`.
#[derive(Debug, Error, uniffi::Error)]
pub enum SignupError {
    /// An underlying API call failed. Contains the specific API error message.
    #[error("{0}")]
    Api(String),

    /// A crypto error occurred.
    #[error("{0}")]
    Crypto(String),

    /// Signup is blocked (e.g., anti-abuse).
    #[error("Signup blocked by server")]
    SignupBlockedByServer,

    /// Username is unavailable.
    #[error("Username unavailable")]
    UsernameUnavailable,

    #[error("Username empty")]
    UsernameEmpty,

    #[error("Password empty")]
    PasswordEmpty,

    #[error("Passwords do not match")]
    PasswordsNotMatching,

    /// Account creation step failed.
    #[error("Account creation failed")]
    AccountCreationFailed,

    /// Address setup step failed.
    #[error("Address setup failed")]
    AddressSetupFailed,

    /// Key setup step failed.
    #[error("Key setup failed")]
    KeySetupFailed,

    /// An unexpected internal error occurred.
    #[error("Internal error")]
    Internal,

    /// The recovery email format is invalid
    #[error("Recovery email format is invalid")]
    RecoveryEmailInvalid,

    /// The recovery phone number format is invalid
    #[error("Recovery phone number format is invalid")]
    RecoveryPhoneNumberInvalid,
}

impl From<RealSignupError> for SignupError {
    fn from(err: RealSignupError) -> Self {
        match err {
            RealSignupError::Api(err) => Self::Api(err.to_string()),
            RealSignupError::Crypto(msg) => Self::Crypto(msg.to_string()),
            RealSignupError::SignupBlockedByServer => Self::SignupBlockedByServer,
            RealSignupError::UsernameUnavailable => Self::UsernameUnavailable,
            RealSignupError::AccountCreationFailed => Self::AccountCreationFailed,
            RealSignupError::AddressSetupFailed => Self::AddressSetupFailed,
            RealSignupError::KeySetupFailed => Self::KeySetupFailed,
            RealSignupError::SetAuthInfoFailed(_) => Self::Internal,
            RealSignupError::SetUserDataFailed(_) => Self::Internal,
            RealSignupError::InvalidState => Self::Internal,
            RealSignupError::RecoveryEmailInvalid => Self::RecoveryEmailInvalid,
            RealSignupError::RecoveryPhoneNumberInvalid => Self::RecoveryPhoneNumberInvalid,
        }
    }
}

impl From<JoinError> for SignupError {
    fn from(_: JoinError) -> Self {
        Self::Internal
    }
}

#[derive(uniffi::Enum, Debug)]
pub enum SimpleSignupState {
    WantUsername,
    WantPassword,
    WantRecovery,
    WantCreate,
    Complete,
    Invalid,
}

impl From<StateKind> for SimpleSignupState {
    fn from(kind: StateKind) -> Self {
        match kind {
            StateKind::WantUsername => Self::WantUsername,
            StateKind::WantPassword => Self::WantPassword,
            StateKind::WantRecovery => Self::WantRecovery,
            StateKind::WantCreate => Self::WantCreate,
            StateKind::Complete => Self::Complete,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct Country {
    pub country_code: String,
    pub country_en: String,
    pub phone_code: u32,
}

impl From<&RealCountry> for Country {
    fn from(country: &RealCountry) -> Self {
        Self {
            country_code: country.country_code.to_owned(),
            country_en: country.country_en.to_owned(),
            phone_code: country.phone_code,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct Countries {
    pub countries: Vec<Country>,
    pub default_country: Option<Country>,
}

/// User activity during text input.
#[derive(uniffi::Record, Clone)]
pub struct UserBehavior {
    /// Time from form load to user providing input (in seconds).
    pub time_on_field: Vec<u32>,
    /// Number of clicks / taps during user input.
    pub click_on_field: u32,
    /// Text chunks copied during user input.
    pub copy_field: Vec<String>,
    /// Text chunks pasted during user input.
    pub paste_field: Vec<String>,
    /// Characters entered during user input.
    pub key_down_field: Vec<String>,
}

impl From<UserBehavior> for RealUserBehavior {
    fn from(value: UserBehavior) -> Self {
        Self {
            time_on_field: value.time_on_field,
            click_on_field: value.click_on_field,
            copy_field: value.copy_field,
            paste_field: value.paste_field,
            key_down_field: value.key_down_field,
        }
    }
}

/// Manages the state and transitions for the user signup process.
#[derive(uniffi::Object)]
pub struct SignupFlow {
    flow: Arc<Mutex<RealSignupFlow>>,
}

impl SignupFlow {
    #[must_use]
    pub fn new(real_flow: RealSignupFlow) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(real_flow)),
        })
    }
}

#[derive(uniffi::Record)]
pub struct UserAddrId {
    pub user_id: String,
    pub addr_id: String,
}

#[uniffi_export]
impl SignupFlow {
    /// Step the flow back to the previous State.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no state to step back to.
    pub async fn step_back(&self) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move { flow.lock().await.back().map_err(SignupError::from) }).await?;

        Ok(self.get_state())
    }

    /// Submit an internal Proton username.
    pub async fn submit_internal_username(
        &self,
        username: String,
        domain: String,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_internal_username(username, domain)
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit an external email address.
    pub async fn submit_external_username(
        &self,
        email: String,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_external_username(email)
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit password.
    pub async fn submit_password(
        &self,
        password: String,
        confirm_password: String,
    ) -> Result<SimpleSignupState, SignupError> {
        if password.trim().is_empty() {
            return Err(SignupError::PasswordEmpty);
        }

        if password != confirm_password {
            return Err(SignupError::PasswordsNotMatching);
        }

        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_password(password)
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit a recovery email address.
    pub async fn submit_recovery_email(
        &self,
        email: String,
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_recovery_email(email, user_behavior.map(|b| b.into()))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit a recovery phone number.
    pub async fn submit_recovery_phone(
        &self,
        phone: String,
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_recovery_phone(phone, user_behavior.map(|b| b.into()))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Skip providing recovery information.
    pub async fn skip_recovery(&self) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .skip_recovery()
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Create the account.
    pub async fn create(
        &self,
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .create(user_behavior.map(|b| b.into()))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Complete the signup flow, returning the user ID and address ID.
    pub fn complete(&self) -> Result<UserAddrId, SignupError> {
        async_runtime()
            .block_on(async { self.flow.lock().await.complete().map_err(SignupError::from) })
            .map(|(_, user, addr)| (user.id, addr.id))
            .map(|(user_id, addr_id)| UserAddrId { user_id, addr_id })
    }

    /// Get the current state of the SignupFlow
    #[must_use]
    pub fn get_state(&self) -> SimpleSignupState {
        async_runtime().block_on(async { self.flow.lock().await.kind().unwrap().into() })
    }

    /// Get the list of available domains (e.g., "proton.me", "protonmail.com").
    #[must_use]
    pub async fn available_domains(&self) -> Result<Vec<String>, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move { Ok(flow.lock().await.available_domains().to_owned()) }).await
    }

    /// Get the list of available countries.
    #[must_use]
    pub async fn available_countries(
        &self,
        default_country_code: String,
    ) -> Result<Countries, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            let countries: Vec<Country> = flow
                .lock()
                .await
                .available_countries()
                .into_iter()
                .map_into()
                .collect_vec();

            let default_country = countries
                .iter()
                .find(|c| c.country_code.eq_ignore_ascii_case(&default_country_code))
                .cloned();

            Ok(Countries {
                countries,
                default_country,
            })
        })
        .await
    }
}
