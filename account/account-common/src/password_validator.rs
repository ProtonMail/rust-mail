use secrecy::{ExposeSecret, SecretString};

pub trait PasswordValidator: Send + Sync {
    fn validate(
        &self,
        password: &SecretString,
        user_id: &Option<String>,
    ) -> PasswordValidatorResult;
}

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

impl PasswordValidator for MinLengthPasswordValidator {
    fn validate(
        &self,
        password: &SecretString,
        _user_id: &Option<String>,
    ) -> PasswordValidatorResult {
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

        let result = validator.validate(&password, &None);
        assert!(!result.hide_if_valid);
        assert!(!result.is_optional);
        assert!(result.is_valid);
    }

    #[test]
    fn min_password_length_too_short() {
        let password = SecretString::from("passwrd".to_string());
        let validator = MinLengthPasswordValidator::default();

        let result = validator.validate(&password, &None);
        assert!(!result.hide_if_valid);
        assert!(!result.is_optional);
        assert!(!result.is_valid);
    }
}
