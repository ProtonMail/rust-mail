use crate::{Error, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use proton_calendar_api::CalendarBootstrap;
use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, DetachedSignatureVariant, Encryptor,
    EncryptorSync, KeyGenerator, KeyGeneratorSync, PGPMessage, PGPProviderSync, Signer, SignerSync,
    VerifiedData,
};
use proton_crypto_account::keys::{UnlockedAddressKey, UnlockedAddressKeys};
use std::borrow::Cow;
use zeroize::Zeroizing;

/// Calendar key, locked behind a passphrase (aka "exported").
///
/// This key is encrypted using a passphrase which itself is encrypted using the
/// address key, so the flow goes:
///
/// - users logs in, unlocking their private key,
/// - you fetch calendar bootstrap data,
/// - you call [`LockedCalendarKey::from_bootstrap()`] to extract calendar key
///   and import it into your favourite PGP provider.
///
/// See: [`UnlockedCalendarKey`].
#[derive(Debug)]
pub struct LockedCalendarKey<'a> {
    key: Cow<'a, str>,
    passphrase: Cow<'a, str>,
    signature: Cow<'a, str>,
}

impl<'a> LockedCalendarKey<'a> {
    pub fn from_bootstrap(calendar: &'a CalendarBootstrap) -> Result<Self> {
        let key = &calendar
            .primary_key()
            .ok_or(Error::CouldntFindPrimaryCalendarKey)?
            .private_key;

        let member = &calendar
            .passphrase
            .for_member(&calendar.member().id)
            .ok_or_else(|| {
                Error::CouldntFindCalendarPassphrase(calendar.member().id.to_string())
            })?;

        Ok(Self {
            key: Cow::Borrowed(key),
            passphrase: Cow::Borrowed(&member.passphrase),
            signature: Cow::Borrowed(&member.signature),
        })
    }

    /// Returns the private key, encrypted and armored.
    ///
    /// ```text
    /// -----BEGIN PGP PRIVATE KEY BLOCK-----
    /// ...
    /// -----END PGP PRIVATE KEY BLOCK-----
    /// ```
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the passphrase used to encrypt the private key, encrypted and
    /// armored.
    ///
    /// ```text
    /// -----BEGIN PGP MESSAGE-----
    /// ...
    /// -----END PGP MESSAGE-----
    /// ```
    #[must_use]
    pub fn passphrase(&self) -> &str {
        &self.passphrase
    }

    /// Returns the signature of the passphrase message, armored.
    ///
    /// ```text
    /// -----BEGIN PGP SIGNATURE-----
    /// ...
    /// -----END PGP SIGNATURE-----
    /// ```
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn import<P>(
        self,
        pgp: &P,
        address_keys: &UnlockedAddressKeys<P>,
    ) -> Result<UnlockedCalendarKey<P>>
    where
        P: PGPProviderSync,
    {
        let passphrase = pgp
            .new_decryptor()
            .with_decryption_key_refs(address_keys)
            .with_verification_key_refs(address_keys)
            .with_detached_signature_ref(
                self.signature().as_bytes(),
                DetachedSignatureVariant::Plaintext,
                true,
            )
            .decrypt(self.passphrase(), DataEncoding::Armor)
            .map_err(Error::CouldntDecryptCalendarPassphrase)?;

        passphrase
            .verification_result()
            .map_err(Error::CouldntVerifyCalendarPassphrase)?;

        let private_key = pgp
            .private_key_import(self.key(), passphrase, DataEncoding::Armor)
            .map_err(Error::CouldntImportCalendarPrivateKey)?;

        UnlockedCalendarKey::wrap(pgp, private_key)
    }
}

/// Calendar key pair, required for working with calendar events.
///
/// See: [`LockedCalendarKey`].
#[derive(Debug)]
pub struct UnlockedCalendarKey<P>
where
    P: PGPProviderSync,
{
    pub private_key: P::PrivateKey,
    pub public_key: P::PublicKey,
}

impl<P> UnlockedCalendarKey<P>
where
    P: PGPProviderSync,
{
    pub fn wrap(pgp: &P, private_key: P::PrivateKey) -> Result<Self> {
        let public_key = pgp
            .private_key_to_public_key(&private_key)
            .map_err(Error::CouldntConvertCalendarPrivateKeyToPublic)?;

        Ok(Self {
            private_key,
            public_key,
        })
    }

    pub fn generate(pgp: &P) -> Result<Self> {
        let private_key = pgp
            .new_key_generator()
            .with_user_id("Calendar key", "Calendar key")
            .generate()
            .map_err(Error::CouldntGenerateCalendarKey)?;

        Self::wrap(pgp, private_key)
    }

    pub fn export(
        &self,
        pgp: &P,
        address_key: &UnlockedAddressKey<P>,
    ) -> Result<LockedCalendarKey<'static>> {
        let passphrase = Zeroizing::new(proton_crypto::generate_secure_random_bytes::<32>());
        let passphrase = Zeroizing::new(BASE64_STANDARD.encode(&passphrase));

        let key = String::from_utf8(
            pgp.private_key_export(&self.private_key, &passphrase, DataEncoding::Armor)
                .map_err(Error::CouldntExportCalendarPrivateKey)?
                .as_ref()
                .to_vec(),
        );

        let signature = String::from_utf8(
            pgp.new_signer()
                .with_signing_key(address_key.as_ref())
                .with_utf8()
                .sign_detached(&passphrase, DataEncoding::Armor)
                .map_err(Error::CouldntSignCalendarPassphrase)?,
        );

        let passphrase = String::from_utf8(
            pgp.new_encryptor()
                .with_encryption_key(address_key.as_public_key())
                .with_utf8()
                .encrypt(&passphrase)
                .map_err(Error::CouldntEncryptCalendarPassphrase)?
                .armor()
                .map_err(Error::CouldntArmorCalendarPassphrase)?,
        );

        // Unwrap-safety: All three are armor-encoded
        let key = key.unwrap();
        let passphrase = passphrase.unwrap();
        let signature = signature.unwrap();

        Ok(LockedCalendarKey {
            key: Cow::Owned(key),
            passphrase: Cow::Owned(passphrase),
            signature: Cow::Owned(signature),
        })
    }
}

impl<P> AsRef<P::PrivateKey> for UnlockedCalendarKey<P>
where
    P: PGPProviderSync,
{
    fn as_ref(&self) -> &P::PrivateKey {
        &self.private_key
    }
}

impl<P> AsPublicKeyRef<P::PublicKey> for UnlockedCalendarKey<P>
where
    P: PGPProviderSync,
{
    fn as_public_key(&self) -> &P::PublicKey {
        &self.public_key
    }
}
