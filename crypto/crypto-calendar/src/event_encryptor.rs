use crate::{EncryptedIcs, Error, KeyPacket, KeyPackets, Result, Signature, UnlockedCalendarKey};
use proton_crypto::crypto::{
    DataEncoding, Encryptor, EncryptorSync, PGPMessage, PGPProviderSync, SessionKeyAlgorithm,
    Signer, SignerSync,
};
use proton_crypto_account::keys::UnlockedAddressKeys;

#[derive(Debug)]
pub struct CalendarEventEncryptor<'a, P>
where
    P: PGPProviderSync,
{
    mode: Mode,
    session_key: P::SessionKey,
    signing_key: &'a P::PrivateKey,
    encryption_key: &'a P::PublicKey,
}

impl<'a, P> CalendarEventEncryptor<'a, P>
where
    P: PGPProviderSync,
{
    pub fn for_address(pgp: &P, address_keys: &'a UnlockedAddressKeys<P>) -> Result<Self> {
        let address_key = address_keys
            .primary_default()
            .ok_or(Error::CouldntFindPrimaryAddressKey)?;

        Self::new(
            pgp,
            Mode::ForAddress,
            &address_key.private_key,
            &address_key.public_key,
        )
    }

    pub fn for_calendar(
        pgp: &P,
        address_keys: &'a UnlockedAddressKeys<P>,
        calendar_key: &'a UnlockedCalendarKey<P>,
    ) -> Result<Self> {
        let address_key = address_keys
            .primary_default()
            .ok_or(Error::CouldntFindPrimaryAddressKey)?;

        Self::new(
            pgp,
            Mode::ForCalendar,
            &address_key.private_key,
            &calendar_key.public_key,
        )
    }

    fn new(
        pgp: &P,
        mode: Mode,
        signing_key: &'a P::PrivateKey,
        encryption_key: &'a P::PublicKey,
    ) -> Result<Self> {
        let session_key = pgp
            .session_key_generate(SessionKeyAlgorithm::default())
            .map_err(Error::CouldntGenerateSessionKey)?;

        Ok(Self {
            mode,
            session_key,
            signing_key,
            encryption_key,
        })
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn encrypt(&self, pgp: &P, ics: &[u8]) -> Result<(EncryptedIcs, Signature)> {
        let sig = pgp
            .new_signer()
            .with_signing_key(self.signing_key)
            .sign_detached(ics, DataEncoding::Armor)
            .map_err(Error::CouldntSignIcs)?;

        // Unwrap-safety: String is armor-encoded
        let sig = String::from_utf8(sig).unwrap();
        let sig = Signature::from_armored(sig);

        let ics = pgp
            .new_encryptor()
            .with_session_key_ref(&self.session_key)
            .encrypt(ics)
            .map_err(Error::CouldntEncryptIcs)?
            .as_data_packet()
            .to_vec();

        let ics = EncryptedIcs::from_bytes(&ics);

        Ok((ics, sig))
    }

    pub fn finish(self, pgp: &P) -> Result<KeyPackets<KeyPacket>> {
        let key_packet = pgp
            .new_encryptor()
            .with_encryption_key(self.encryption_key)
            .encrypt_session_key(&self.session_key)
            .map_err(Error::CouldntEncryptSessionKey)?;

        let key_packet = KeyPacket::from_bytes(&key_packet);

        match self.mode {
            Mode::ForAddress => Ok(KeyPackets {
                address_key_packet: Some(key_packet),
                shared_key_packet: None,
            }),
            Mode::ForCalendar => Ok(KeyPackets {
                address_key_packet: None,
                shared_key_packet: Some(key_packet),
            }),
        }
    }
}

#[derive(Debug)]
enum Mode {
    ForAddress,
    ForCalendar,
}
