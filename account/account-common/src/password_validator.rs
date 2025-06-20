use proton_account_api::{
    AccountApi, ApiError,
    prelude::{PasswordPolicyResponse, PasswordPolicyState},
};
use regex::Regex;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tracing::info;

pub struct PasswordValidatorResult {
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

impl PasswordValidatorResult {
    /// If true, the validation was successful.
    /// Note: a validation may be successful, even if it's not valid, in case it's optional.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.is_valid || self.is_optional
    }
}

pub struct PasswordValidatorService {
    pub client: muon::Client,
    pub policies: Vec<BackendPasswordValidator>,
}

impl PasswordValidatorService {
    pub async fn fetch_validators(&mut self) -> Result<(), FetchValidatorsError> {
        info!("Fetching password policies");
        let result = self
            .client
            .get_password_policies()
            .await
            .map_err(FetchValidatorsError::Api)?;

        self.policies = result
            .password_policies
            .into_iter()
            .map(|policy| {
                let regex = Regex::new(&policy.regex)
                    .map_err(|e| FetchValidatorsError::Regex(e.to_string()));
                regex.map(|regex| BackendPasswordValidator { policy, regex })
            })
            .collect::<Result<Vec<_>, _>>()?;

        info!(
            "{} password policies has been fetched successfully",
            self.policies.len()
        );
        Ok(())
    }
    #[must_use]
    pub fn validate(&self, password: &SecretString) -> Vec<PasswordValidatorResult> {
        if self.policies.is_empty() {
            // There are no policies fetched from the backend yet, use the default validator
            vec![MinLengthPasswordValidator::default().validate(password)]
        } else {
            self.policies.iter().map(|v| v.validate(password)).collect()
        }
    }
}

pub struct BackendPasswordValidator {
    pub policy: PasswordPolicyResponse,
    /// The regex. It should be applied to the password. If it returns true, the policy passed.
    pub regex: Regex,
}

impl BackendPasswordValidator {
    fn validate(&self, password: &SecretString) -> PasswordValidatorResult {
        PasswordValidatorResult {
            error_message: self.policy.error_message.clone(),
            hide_if_valid: self.policy.hide_if_valid,
            is_optional: self.policy.state == PasswordPolicyState::Optional,
            is_valid: self.regex.is_match(password.expose_secret()),
            requirement_message: self.policy.requirement_message.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum FetchValidatorsError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("Regex error: {0}")]
    Regex(String),
}

pub struct MinLengthPasswordValidator {
    min_length: usize,
}

impl MinLengthPasswordValidator {
    fn new(min_length: usize) -> Self {
        Self { min_length }
    }
}

impl Default for MinLengthPasswordValidator {
    fn default() -> Self {
        Self::new(8)
    }
}

impl MinLengthPasswordValidator {
    fn validate(&self, password: &SecretString) -> PasswordValidatorResult {
        PasswordValidatorResult {
            error_message: format!("MIN_LENGTH_{}", self.min_length),
            hide_if_valid: false,
            is_optional: false,
            is_valid: password.expose_secret().chars().count() >= self.min_length,
            requirement_message: format!("MIN_LENGTH_{}", self.min_length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_password_length_valid() {
        let password = SecretString::from("password".to_string());
        let validator = MinLengthPasswordValidator::default();

        let result = validator.validate(&password);
        assert!(!result.hide_if_valid);
        assert!(!result.is_optional);
        assert!(result.is_valid);
    }

    #[test]
    fn min_password_length_too_short() {
        let password = SecretString::from("passwrd".to_string());
        let validator = MinLengthPasswordValidator::default();

        let result = validator.validate(&password);
        assert!(!result.hide_if_valid);
        assert!(!result.is_optional);
        assert!(!result.is_valid);
    }
}
