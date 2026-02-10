use proton_crypto_account::{keys::ContactType, proton_crypto::crypto::OpenPGPFingerprint};

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum EncryptionPreferencesError {
    #[error("Internal user with no valid API keys")]
    InternalUserNoApiKeys,
    #[error("No primary address key for user owned address")]
    NoPrimaryKey,
    #[error(
        "Invalid selected key for {0} recipient with fingerprint {1} (obsolete: {2}, compromised: {3}, can encrypt: {4})"
    )]
    SelectedKeyCannotSend(ContactType, OpenPGPFingerprint, bool, bool, bool),
    /// This error is thrown if there are pinned keys, but none of the fingerprints of the pinned keys matches the fingerprint of one of the keys served by the API.
    ///
    /// In this case the client should force the user (via a modal)
    /// to trust one of the keys served by the API before sending any email.
    /// The provided API key fingerprint is a suggestion for which key to trust, but there may be others.
    #[error(
        "No matching API key found for pinned keys, user should add API key with fingerprint {0} to its contact"
    )]
    PinnedKeyNotProvidedByAPI(OpenPGPFingerprint),
    #[error(
        "Invalid pinned key with fingerprint {0} (obsolete: {1}, compromised: {2}, can encrypt: {3})"
    )]
    ExternalUserNoValidPinnedKey(OpenPGPFingerprint, bool, bool, bool),
    #[error("No valid key for encryption found in owned address keys")]
    ExternalUserNoValidApiKey,
}

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum CryptoPackageTypeError {
    #[error("Failed to convert {0} to a package type.")]
    Parse(u8),
}
