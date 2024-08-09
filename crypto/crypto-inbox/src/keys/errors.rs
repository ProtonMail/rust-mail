#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum EncryptionPreferencesError {
    #[error("No key was found for the user")]
    InternalUserWithNoKeys,
    #[error("Invalid primary key (obsolete: {0}, compromised: {1})")]
    InvalidPrimaryKey(bool, bool),
    #[error("No key found for encryption")]
    NoKeyFound,
}

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum CryptoPackageTypeError {
    #[error("Failed to convert {0} to a package type.")]
    Parse(i32),
}
