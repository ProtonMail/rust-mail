use proton_crypto_account::proton_crypto::crypto::OpenPGPFingerprint;

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

#[derive(Debug, Clone, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum UserWarning {
    #[error("No matching API key found for pinned keys, trust API key {0}.")]
    PromptUserToTrust(OpenPGPFingerprint),
    #[error("No valid pinned key found but user has pinned keys.")]
    NoValidPinnedKey,
}
