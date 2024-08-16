use proton_crypto_account::proton_crypto::crypto::OpenPGPFingerprint;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum EncryptionPreferencesError {
    #[error("Internal user with no valid API keys")]
    InternalUserNoApiKeys,
    #[error("No primary key for sending found")]
    NoPrimaryKey,
    #[error("Invalid primary key with fingerprint {0} (obsolete: {1}, compromised: {2}, can encrypt: {3})")]
    PrimaryKeyCannotSend(OpenPGPFingerprint, bool, bool, bool),
    #[error("No matching API key found for pinned keys, user should add API key with fingerprint {0} to its contact")]
    PrimaryKeyNotPinned(OpenPGPFingerprint),
    #[error("Invalid pinned key with fingerprint {0} (obsolete: {1}, compromised: {2}, can encrypt: {3})")]
    ExternalUserNoValidPinnedKey(OpenPGPFingerprint, bool, bool, bool),
    #[error("No valid key for encryption found in owned address keys")]
    ExternalUserNoValidApiKey,
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
}
