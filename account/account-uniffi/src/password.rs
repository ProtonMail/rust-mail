use proton_account_api::password::PasswordError as RealPasswordError;
use proton_account_api::password::PasswordFlow as RealPasswordFlow;
use proton_core_api::consts::CoreBundle;
use proton_core_api::service::ApiServiceError;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinError;
use uniffi_runtime::async_runtime;
use uniffi_runtime::uniffi_async;

/// Errors that can occur during the password change flow, exposed via `UniFFI`.
#[derive(Debug, Error, uniffi::Error)]
pub enum PasswordError {
    #[error("API error: {0}")]
    Api(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Invalid 2FA code")]
    Invalid2FACode,

    #[error("Key unlock error")]
    KeyUnlock,

    #[error("Invalid state")]
    InvalidState,

    #[error("Internal error")]
    Internal,
}

impl From<RealPasswordError> for PasswordError {
    fn from(value: RealPasswordError) -> Self {
        match value {
            RealPasswordError::InvalidState => Self::InvalidState,

            // Auth error caused by invalid 2FA input/token
            RealPasswordError::FlowAuth(ApiServiceError::UnprocessableEntity(_, Some(info)))
                if info.code == CoreBundle::Auth2faInputInvalid as u32
                    || info.code == CoreBundle::Auth2faTokenInvalid as u32 =>
            {
                Self::Invalid2FACode
            }

            // Auth error caused by incorrect password
            RealPasswordError::FlowAuth(ApiServiceError::UnprocessableEntity(..))
            | RealPasswordError::KeySecretSaltFetch(ApiServiceError::UnprocessableEntity(..))
            | RealPasswordError::ServerProof => Self::InvalidCredentials,

            // Key unlock error
            RealPasswordError::MissingPrimaryKey
            | RealPasswordError::KeySecretDecryption
            | RealPasswordError::KeySecretDerivation(_)
            | RealPasswordError::KeyEncoding(_) => Self::KeyUnlock,

            // Api service error
            RealPasswordError::ApiService(e)
            | RealPasswordError::KeySecretSaltFetch(e)
            | RealPasswordError::FlowAuth(e) => e
                .to_proton_error()
                .and_then(|e| e.error)
                .map_or_else(|| Self::Api(e.to_string()), Self::Api),

            // Api error
            RealPasswordError::Api(e) => e
                .body_str()
                .map_or_else(|| Self::Api(e.to_string()), |e| Self::Api(e.to_owned())),

            // Crypto error
            RealPasswordError::Crypto(e) => Self::Crypto(e.to_string()),

            // Store error
            RealPasswordError::Store(_) => Self::Internal,
        }
    }
}

impl From<JoinError> for PasswordError {
    fn from(_: JoinError) -> Self {
        Self::Internal
    }
}

/// Manages the password change process for a user.
#[derive(uniffi::Object)]
pub struct PasswordFlow {
    flow: Arc<Mutex<RealPasswordFlow>>,
}

impl PasswordFlow {
    #[must_use]
    pub fn new(flow: RealPasswordFlow) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
        })
    }
}

#[uniffi_export]
impl PasswordFlow {
    /// Submit the current password to start the password change.
    ///
    /// # Arguments
    /// * `password` - The current password to submit
    ///
    /// # Errors
    pub async fn submit_password(&self, password: String) -> Result<(), PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            flow.lock()
                .await
                .submit_password(password)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(())
    }

    /// Submit TOTP code for 2FA authentication.
    ///
    /// # Arguments
    /// * `code` - The TOTP code from the authenticator app
    ///
    /// # Errors
    /// Returns an error if the TOTP code is invalid.
    pub async fn submit_totp(&self, code: String) -> Result<(), PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            flow.lock()
                .await
                .submit_totp(code)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(())
    }

    /// Submit FIDO2 response for 2FA authentication.
    ///
    /// # Arguments
    /// * `response` - The FIDO2 response from the security key
    ///
    /// # Errors
    /// Returns an error if the FIDO2 response is invalid.
    pub async fn submit_fido2(&self, response: String) -> Result<(), PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            flow.lock()
                .await
                .submit_fido2(response)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(())
    }

    /// Submit the new password to complete the password change.
    ///
    /// # Arguments
    /// * `password` - The new password to set
    ///
    /// # Errors
    /// Returns an error if the password change fails.
    pub async fn submit_new_password(&self, password: String) -> Result<(), PasswordError> {
        let flow = self.flow.clone();

        uniffi_async::<_, PasswordError, _>(async move {
            flow.lock()
                .await
                .submit_new_password(password)
                .await
                .map_err(PasswordError::from)
        })
        .await?;

        Ok(())
    }

    /// Check if the flow is awaiting password.
    #[must_use]
    pub fn is_awaiting_password(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_password() })
    }

    /// Check if the flow is awaiting 2FA authentication.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_2fa() })
    }

    /// Check if the flow is awaiting the new password.
    #[must_use]
    pub fn is_awaiting_new_password(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_new_password() })
    }

    /// Check if the flow is complete.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_complete() })
    }
}
