use std::sync::Arc;

use muon::http::DynHttpSender;
use proton_account_common::password_validator::PasswordType as RealPasswordType;
use proton_account_common::password_validator::PasswordValidatorResult;
use proton_account_common::password_validator::PasswordValidatorService as RealPasswordValidatorService;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use tokio::task::JoinError;
use tracing::error;
use uniffi_runtime::async_runtime;

#[derive(uniffi::Object)]
pub struct PasswordValidatorService {
    service: Arc<Mutex<RealPasswordValidatorService>>,
}

impl PasswordValidatorService {
    /// Creates a new service, while spawning an async task to fetch password policies from the API.
    /// This method returns immediately, without waiting for the spawned task.
    #[must_use]
    pub fn setup(api: DynHttpSender) -> PasswordValidatorService {
        let real_service = Arc::new(Mutex::new(RealPasswordValidatorService::new(api)));
        let real_service_clone = real_service.clone();
        async_runtime().spawn(async move {
            let mut guard = real_service_clone.lock().await;
            match guard.fetch_validators().await {
                Ok(()) => (),
                Err(err) => error!("Cannot fetch password validators: {err}"),
            }
        });
        PasswordValidatorService {
            service: real_service,
        }
    }
}

#[uniffi::export]
impl PasswordValidatorService {
    #[must_use]
    pub fn validate(
        &self,
        password_type: PasswordType,
        plain_password: String,
        callback: Box<dyn PasswordValidatorServiceCallback>,
    ) -> PasswordValidatorServiceHandle {
        async_runtime().block_on(async {
            let password = SecretString::from(plain_password);
            let service = self.service.clone();
            let guard = service.lock().await;
            let results = guard.validate(password_type.into(), &password);
            let token = results
                .iter()
                .all(PasswordValidatorResult::is_success)
                .then_some(Arc::new(PasswordValidatorServiceToken::new(
                    password_type,
                    password,
                )));
            let handle = async_runtime().spawn(async move {
                callback.on_results(results.into_iter().map(to_service_result).collect(), token);
            });
            PasswordValidatorServiceHandle {
                handle: handle.abort_handle(),
            }
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq, uniffi::Enum)]
pub enum PasswordType {
    /// Main login password.
    Main,
    /// Secondary (mailbox) password.
    Secondary,
}

impl From<PasswordType> for RealPasswordType {
    fn from(value: PasswordType) -> Self {
        match value {
            PasswordType::Main => RealPasswordType::Main,
            PasswordType::Secondary => RealPasswordType::Secondary,
        }
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
    /// The type of the validated password.
    password_type: PasswordType,
    /// The password that has been validated.
    validated_password: SecretString,
}

impl PasswordValidatorServiceToken {
    fn new(password_type: PasswordType, validated_password: SecretString) -> Self {
        Self {
            password_type,
            validated_password,
        }
    }

    #[must_use]
    pub fn matches(&self, password_type: PasswordType, password: &String) -> bool {
        self.validated_password.expose_secret() == password && self.password_type == password_type
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
        let t = PasswordValidatorServiceToken::new(
            PasswordType::Main,
            SecretString::from("password".to_string()),
        );
        assert!(!t.matches(PasswordType::Main, &"pass".to_string()));
        assert!(!t.matches(PasswordType::Main, &"password1".to_string()));
        assert!(t.matches(PasswordType::Main, &"password".to_string()));
        assert!(!t.matches(PasswordType::Secondary, &"password".to_string()));
    }
}
