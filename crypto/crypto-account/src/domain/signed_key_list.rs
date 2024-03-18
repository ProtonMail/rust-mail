use std::fmt::{Display, Formatter};

use crate::errors::SKLError;

use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, OpenPGPFingerprint, PGPProviderAsync, PGPProviderSync,
    SHA256Fingerprint, UnixTimestamp, Verifier, VerifierAsync, VerifierSync,
};
use serde::{Deserialize, Serialize};

use super::{KeyFlag, ProtonBoolean};

pub const KT_SKL_VERIFICATION_CONTEXT_VALUE: &str = "key-transparency.key-list";

crate::string_id! {
    /// A Signed Key List (SKL) signature.
    SKLSignature
}

impl AsRef<[u8]> for SKLSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

crate::string_id! {
    /// A Signed Key List (SKL) signature.
    ObsolescenceToken
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, Hash)]
#[serde(rename_all = "PascalCase")]
/// The key `data` of singed key list.
pub struct SignedKeyListData {
    pub fingerprint: OpenPGPFingerprint,
    #[serde(rename = "SHA256Fingerprints")]
    pub sha265_fingerprints: Vec<SHA256Fingerprint>,
    pub flags: KeyFlag,
    pub primary: ProtonBoolean,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, Hash)]
#[serde(rename_all = "PascalCase")]
/// Represents a signed key list fetched from the API.
pub struct SignedKeyList {
    #[serde(rename = "MinEpochID")]
    /// Starting Epoch ID for the SKL. Can be None, if the epoch is not yet released.
    pub min_epoch_id: Option<u64>,
    #[serde(rename = "MaxEpochID")]
    /// Ending Epoch ID for the SKL. Can be None, if the epoch is not yet released
    pub max_epoch_id: Option<u64>,
    #[serde(rename = "ExpectedMinEpochID")]
    /// If epoch is not yet released this will be a future epoch ID.
    pub expected_min_epoch_id: Option<u64>,
    /// JSON-encoded content of the SKL (`SignedKeyListData`). If None, this SKL contains an ObsolescenceToken
    pub data: Option<String>,
    /// Hex token to prove the obsolescence of the signed key list in the merkle tree or None.
    ///
    /// The first 16 characters are a committed big-endian hex-encoded unix timestamp, remaining is random
    pub obsolescence_token: Option<ObsolescenceToken>,
    /// Armored OpenPGP signature for the data. If None, proof contains an obsolescenceToken
    pub signature: Option<SKLSignature>,
    /// SKL revision version.
    ///
    /// First revision is 1, then monotonically increasing.
    pub revision: u64,
}

impl SignedKeyList {
    /// Returns if the SKL is released in an epoch.
    pub fn is_released_in_epoch(&self) -> bool {
        self.max_epoch_id.is_some() && self.max_epoch_id.is_some()
    }
    /// Returns if the SKL represents an obsolete address.
    pub fn is_obsolescence(&self) -> bool {
        self.obsolescence_token.is_some()
    }
    /// Returns if the SKL represents an address with active address keys.
    pub fn is_active(&self) -> bool {
        self.signature.is_some() && self.data.is_some() && !self.is_obsolescence()
    }

    /// Returns if the SKL represents an address with active address keys.
    pub fn signed_key_list_data(&self) -> Result<Vec<SignedKeyListData>, SKLError> {
        let data = self.data.as_ref().ok_or(SKLError::NoSKLData)?;
        let skl_data: Vec<SignedKeyListData> =
            serde_json::from_str(data.as_str()).map_err(|err| SKLError::ParseError(err.into()))?;
        Ok(skl_data)
    }

    /// Verifies the included SKL signature.
    pub fn verify_signature<Prov: PGPProviderSync>(
        &self,
        provider: &Prov,
        verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
        verification_time: Option<UnixTimestamp>,
    ) -> Result<UnixTimestamp, SKLError> {
        let (Some(data), Some(signature)) = (&self.data, &self.signature) else {
            return Err(SKLError::NoSKLData);
        };
        let verification_context = provider.new_verification_context(
            KT_SKL_VERIFICATION_CONTEXT_VALUE.to_string(),
            false,
            UnixTimestamp::default(),
        );
        let mut verifier = provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .with_verification_context(&verification_context)
            .with_utf8_out();
        if let Some(timestamp) = verification_time {
            verifier = verifier.at_verification_time(timestamp);
        };
        verifier
            .verify_detached(data.as_bytes(), signature, DataEncoding::Armor)
            .map(|info| info.signature_creation_time)
            .map_err(SKLError::SignatureVerificationError)
    }

    /// Verifies the included SKL signature.
    pub async fn verify_signature_async<Prov: PGPProviderAsync>(
        &self,
        provider: &Prov,
        verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
        verification_time: Option<UnixTimestamp>,
    ) -> Result<UnixTimestamp, SKLError> {
        let (Some(data), Some(signature)) = (&self.data, &self.signature) else {
            return Err(SKLError::NoSKLData);
        };
        let verification_context = provider.new_verification_context(
            KT_SKL_VERIFICATION_CONTEXT_VALUE.to_string(),
            false,
            UnixTimestamp::default(),
        );
        let mut verifier = provider
            .new_verifier_async()
            .with_verification_key_refs(verification_keys)
            .with_verification_context(&verification_context)
            .with_utf8_out();
        if let Some(timestamp) = verification_time {
            verifier = verifier.at_verification_time(timestamp);
        };
        verifier
            .verify_detached_async(data.as_bytes(), signature, DataEncoding::Armor)
            .await
            .map(|info| info.signature_creation_time)
            .map_err(SKLError::SignatureVerificationError)
    }
}
