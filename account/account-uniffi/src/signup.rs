#![allow(clippy::match_same_arms)]

//! Implements the sign-up flow.

use crate::login::PostLoginValidationError;
use crate::password_validator::PasswordType;
use crate::password_validator::PasswordValidatorService;
use crate::user_behavior::UserBehavior;
use itertools::Itertools;
use muon::common::IntoDyn;
use proton_account_api::countries::Country as RealCountry;
use proton_account_api::signup::SignupError as RealSignupError;
use proton_account_api::signup::SignupFlow as RealSignupFlow;
use proton_account_api::signup::state::StateKind;
use proton_core_common::post_login_check::PostLoginValidationError as RealPostLoginValidationError;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use uniffi_runtime::{async_runtime, uniffi_async};

use crate::password_validator::PasswordValidatorServiceToken;

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
    UsernameUnavailable(Option<String>),

    #[error("Username empty")]
    UsernameEmpty,

    #[error("Password empty")]
    PasswordEmpty,

    #[error("Passwords do not match")]
    PasswordsNotMatching,

    #[error("Password validation mismatch")]
    PasswordValidationMismatch,

    #[error("Password not validated")]
    PasswordNotValidated,

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

    /// The recovery phone number format is invalid
    #[error("Post login check failed: {0:?}")]
    PostLoginValidationError(PostLoginValidationError),
}

impl From<RealSignupError> for SignupError {
    fn from(err: RealSignupError) -> Self {
        match err {
            RealSignupError::Api(err) => Self::Api(err.to_string()),
            RealSignupError::Crypto(msg) => Self::Crypto(msg.to_string()),
            RealSignupError::SignupBlockedByServer => Self::SignupBlockedByServer,
            RealSignupError::UsernameUnavailable(msg) => Self::UsernameUnavailable(msg),
            RealSignupError::AccountCreationFailed => Self::AccountCreationFailed,
            RealSignupError::AddressSetupFailed => Self::AddressSetupFailed,
            RealSignupError::KeySetupFailed => Self::KeySetupFailed,
            RealSignupError::SetAuthInfoFailed(_) => Self::Internal,
            RealSignupError::SetUserDataFailed(_) => Self::Internal,
            RealSignupError::InvalidState => Self::Internal,
            RealSignupError::RecoveryEmailInvalid => Self::RecoveryEmailInvalid,
            RealSignupError::RecoveryPhoneNumberInvalid => Self::RecoveryPhoneNumberInvalid,
            RealSignupError::PostLoginCheckFailed(RealPostLoginValidationError::DelinquentUser) => {
                Self::PostLoginValidationError(PostLoginValidationError::DelinquentUser)
            }
            RealSignupError::PostLoginCheckFailed(
                RealPostLoginValidationError::FreeAccountLimitExceeded(limit),
            ) => Self::PostLoginValidationError(
                PostLoginValidationError::FreeAccountLimitExceeded(limit),
            ),
            RealSignupError::PostLoginCheckFailed(RealPostLoginValidationError::Other(_err)) => {
                Self::Internal
            }
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
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_internal_username(username, domain, user_behavior.map(Into::into))
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
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .submit_external_username(email, user_behavior.map(Into::into))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Submit validated password.
    pub async fn submit_password(
        &self,
        password: String,
        confirm_password: String,
        token: Option<Arc<PasswordValidatorServiceToken>>,
    ) -> Result<SimpleSignupState, SignupError> {
        token
            .ok_or(SignupError::PasswordNotValidated)?
            .matches(PasswordType::Main, &password)
            .then_some(())
            .ok_or(SignupError::PasswordValidationMismatch)?;

        self.submit_password_flow(password, confirm_password).await
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
                .submit_recovery_email(email, user_behavior.map(Into::into))
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
                .submit_recovery_phone(phone, user_behavior.map(Into::into))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Skip providing recovery information.
    pub async fn skip_recovery(
        &self,
        user_behavior: Option<UserBehavior>,
    ) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move {
            flow.lock()
                .await
                .skip_recovery(user_behavior.map(Into::into))
                .await
                .map_err(SignupError::from)
        })
        .await?;

        Ok(self.get_state())
    }

    /// Create the account.
    pub async fn create(&self) -> Result<SimpleSignupState, SignupError> {
        let flow = self.flow.clone();

        uniffi_async(async move { flow.lock().await.create().await.map_err(SignupError::from) })
            .await?;

        Ok(self.get_state())
    }

    /// Complete the signup flow, returning the user ID and address ID.
    pub fn complete(&self) -> Result<UserAddrId, SignupError> {
        async_runtime()
            .block_on(async { self.flow.lock().await.complete().map_err(SignupError::from) })
            .map(|(_, user, addr)| (user.id, addr.id))
            .map(|(user_id, addr_id)| UserAddrId {
                user_id: user_id.to_string(),
                addr_id,
            })
    }

    /// Get the current state of the `SignupFlow`
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

    /// Returns a password validator service.
    pub async fn password_validator(&self) -> Option<Arc<PasswordValidatorService>> {
        let flow = self.flow.clone();

        uniffi_async::<_, JoinError, _>(async move {
            Ok(Arc::new(PasswordValidatorService::setup(
                flow.lock().await.api().to_owned().into_dyn(),
            )))
        })
        .await
        .ok()
    }
}

impl SignupFlow {
    async fn submit_password_flow(
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
}
