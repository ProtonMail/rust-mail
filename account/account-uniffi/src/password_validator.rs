use std::sync::Arc;

use proton_account_common::password_validator::MinLengthPasswordValidator;
use proton_account_common::password_validator::PasswordValidator;
use proton_account_common::password_validator::PasswordValidatorResult;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use tokio::task::AbortHandle;
use uniffi_runtime::async_runtime;

#[derive(uniffi::Object)]
pub struct PasswordValidatorService {}

#[uniffi::export]
impl PasswordValidatorService {
    #[must_use]
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    #[must_use]
    pub fn validate(
        &self,
        password: String,
        user_id: Option<String>,
        callback: Box<dyn PasswordValidatorServiceCallback>,
    ) -> PasswordValidatorServiceHandle {
        let local_validators = [Box::new(MinLengthPasswordValidator::default())];
        let password_secret = SecretString::from(password);
        let handle = async_runtime().spawn(async move {
            let results: Vec<PasswordValidatorServiceResult> = local_validators
                .iter()
                .map(|v| to_service_result(v.validate(&password_secret, &user_id)))
                .collect();
            let token = results
                .iter()
                .all(PasswordValidatorServiceResult::is_success)
                .then_some(Arc::new(PasswordValidatorServiceToken::new(
                    password_secret,
                )));
            callback.on_results(results, token);
        });

        PasswordValidatorServiceHandle {
            handle: handle.abort_handle(),
        }
    }
}

impl Default for PasswordValidatorService {
    fn default() -> Self {
        PasswordValidatorService::new()
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

/// Result of password validation.
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

impl PasswordValidatorServiceResult {
    /// If true, the validation was successful.
    /// Note: a validation may be successful, even if it's not valid, in case it's optional.
    fn is_success(&self) -> bool {
        self.is_valid || self.is_optional
    }
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
