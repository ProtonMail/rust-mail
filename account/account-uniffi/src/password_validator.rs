use std::sync::Arc;

use proton_account_common::password_validator::FetchValidatorsError as RealFetchValidatorsError;
use proton_account_common::password_validator::PasswordValidatorResult;
use proton_account_common::password_validator::PasswordValidatorService as RealPasswordValidatorService;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use tokio::task::JoinError;
use uniffi_runtime::async_runtime;
use uniffi_runtime::uniffi_async;

use crate::login::LoginFlow;

#[derive(uniffi::Object)]
pub struct PasswordValidatorService {
    service: Arc<Mutex<RealPasswordValidatorService>>,
}

#[derive(Debug, Error, uniffi::Error)]
pub enum FetchValidatorsError {
    #[error("API error: {0}")]
    Api(String),

    #[error("Regex error: {0}")]
    Regex(String),

    #[error("{0}")]
    Other(String),
}

#[uniffi::export]
impl PasswordValidatorService {
    pub async fn fetch_validators(&self) -> Result<(), FetchValidatorsError> {
        let service = self.service.clone();
        uniffi_async::<_, FetchValidatorsError, _>(async move {
            let mut guard = service.lock().await;
            guard
                .fetch_validators()
                .await
                .map_err(FetchValidatorsError::from)
        })
        .await
    }

    pub async fn validate(
        &self,
        plain_password: String,
        callback: Box<dyn PasswordValidatorServiceCallback>,
    ) -> Result<PasswordValidatorServiceHandle, PasswordValidationError> {
        let password = SecretString::from(plain_password);
        let service = self.service.clone();
        uniffi_async::<_, PasswordValidationError, _>(async move {
            let guard = service.lock().await;
            let results = guard.validate(&password);
            let token = results
                .iter()
                .all(PasswordValidatorResult::is_success)
                .then(|| Arc::new(PasswordValidatorServiceToken::new(password)));
            let handle = async_runtime().spawn(async move {
                callback.on_results(results.into_iter().map(to_service_result).collect(), token);
            });
            Ok(PasswordValidatorServiceHandle {
                handle: handle.abort_handle(),
            })
        })
        .await
    }
}

#[derive(Debug, Error, uniffi::Error)]
pub enum PasswordValidationError {
    #[error("JoinError: {0}")]
    JoinError(String),
}
impl From<JoinError> for PasswordValidationError {
    fn from(value: JoinError) -> Self {
        Self::JoinError(value.to_string())
    }
}

impl PasswordValidatorService {
    pub async fn new(login_flow: &LoginFlow) -> Result<Self, String> {
        let flow = login_flow.inner_flow().clone();

        async_runtime()
            .spawn(async move {
                let guard = flow.lock().await;
                PasswordValidatorService {
                    service: Arc::new(Mutex::new(RealPasswordValidatorService {
                        client: guard.api().clone(),
                        policies: Vec::new(),
                    })),
                }
            })
            .await
            .map_err(|err| err.to_string())
    }
}

#[uniffi::export(callback_interface)]
pub trait PasswordValidatorServiceCallback: Send + Sync {
    /// Called when the validation has been performed.
    /// May be called multiple times.
    /// * `results` - A list of validation results.
    /// * `token` - If present, the validation was successful.
    fn on_results(
        &self,
        results: Vec<PasswordValidatorServiceResult>,
        token: Option<Arc<PasswordValidatorServiceToken>>,
    );
}

/// Represents a confirmation that a given password was validated.
#[derive(uniffi::Object)]
pub struct PasswordValidatorServiceToken {
    /// The password that has been validated.
    validated_password: SecretString,
}

impl PasswordValidatorServiceToken {
    fn new(validated_password: SecretString) -> Self {
        Self { validated_password }
    }

    #[must_use]
    pub fn matches(&self, password: &String) -> bool {
        self.validated_password.expose_secret() == password
    }
}

/// Handle to cancel ongoing password validation.
#[derive(uniffi::Object)]
pub struct PasswordValidatorServiceHandle {
    handle: AbortHandle,
}

#[uniffi::export]
impl PasswordValidatorServiceHandle {
    /// Cancel the ongoing validation.
    pub fn cancel(&self) {
        self.handle.abort();
    }
}

fn to_service_result(result: PasswordValidatorResult) -> PasswordValidatorServiceResult {
    PasswordValidatorServiceResult {
        error_message: result.error_message,
        hide_if_valid: result.hide_if_valid,
        is_optional: result.is_optional,
        is_valid: result.is_valid,
        requirement_message: result.requirement_message,
    }
}

impl From<RealFetchValidatorsError> for FetchValidatorsError {
    fn from(value: RealFetchValidatorsError) -> Self {
        match value {
            RealFetchValidatorsError::Api(e) => FetchValidatorsError::Api(e.to_string()),
            RealFetchValidatorsError::Regex(e) => FetchValidatorsError::Regex(e),
        }
    }
}

impl From<JoinError> for FetchValidatorsError {
    fn from(value: JoinError) -> Self {
        Self::Other(value.to_string())
    }
}

#[derive(uniffi::Record)]
#[allow(clippy::struct_excessive_bools)]
pub struct PasswordValidatorServiceResult {
    /// The message displayed to the user if this validation fails.
    pub error_message: String,
    /// If true, the requirement message should be hidden from the user.
    pub hide_if_valid: bool,
    /// If true, passing this validation is not required to proceed.
    pub is_optional: bool,
    /// If true, then this validation has passed.
    pub is_valid: bool,
    /// The message displayed to the user, explaining what is needed to pass the validation.
    pub requirement_message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matching() {
        let t = PasswordValidatorServiceToken::new(SecretString::from("password".to_string()));
        assert!(!t.matches(&"pass".to_string()));
        assert!(!t.matches(&"password1".to_string()));
        assert!(t.matches(&"password".to_string()));
    }
}
