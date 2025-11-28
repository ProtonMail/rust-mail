use crate::{
    DecryptedIcs, EncryptedIcsRef, Error, KeyPacketRef, KeyPackets, Result, SignatureRef,
    UnlockedCalendarKey,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use derive_more::Debug;
use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, DetachedSignatureVariant,
    PGPProviderSync, VerifiedData,
};
use proton_crypto_account::keys::UnlockedAddressKeys;
use std::iter;

#[derive(Debug)]
pub struct CalendarEventDecryptor<'a, P>
where
    P: PGPProviderSync,
{
    #[debug(skip)]
    session_key: P::SessionKey,

    #[debug(skip)]
    verification_keys: Vec<&'a P::PublicKey>,
}

impl<'a, P> CalendarEventDecryptor<'a, P>
where
    P: PGPProviderSync,
{
    pub fn new(
        pgp: &P,
        address_keys: &'a UnlockedAddressKeys<P>,
        calendar_key: &UnlockedCalendarKey<P>,
        key_packets: KeyPackets<KeyPacketRef>,
    ) -> Result<Self> {
        if let Some(packet) = key_packets.address_key_packet {
            Self::for_address(pgp, address_keys, packet)
        } else if let Some(packet) = key_packets.shared_key_packet {
            Self::for_calendar(pgp, address_keys, calendar_key, packet)
        } else {
            Err(Error::BothKeyPacketsAreMissing)
        }
    }

    pub fn for_address(
        pgp: &P,
        address_keys: &'a UnlockedAddressKeys<P>,
        address_key_packet: KeyPacketRef,
    ) -> Result<Self> {
        Self::new_ex(
            pgp,
            address_key_packet,
            address_keys.iter(),
            address_keys.iter(),
            "address",
        )
    }

    pub fn for_calendar(
        pgp: &P,
        address_keys: &'a UnlockedAddressKeys<P>,
        calendar_key: &UnlockedCalendarKey<P>,
        shared_key_packet: KeyPacketRef,
    ) -> Result<Self> {
        Self::new_ex(
            pgp,
            shared_key_packet,
            iter::once(calendar_key),
            address_keys.iter(),
            "shared",
        )
    }

    fn new_ex<'b, D, V>(
        pgp: &P,
        packet: KeyPacketRef,
        decryption_keys: impl IntoIterator<Item = &'b D>,
        verification_keys: impl IntoIterator<Item = &'a V>,
        ty: &'static str,
    ) -> Result<Self>
    where
        D: AsRef<P::PrivateKey> + 'b,
        V: AsPublicKeyRef<P::PublicKey> + 'a,
    {
        let packet = BASE64_STANDARD
            .decode(packet.as_base64())
            .map_err(|err| Error::CouldntDecodeKeyPacket { ty, err })?;

        let decryption_keys: Vec<_> = decryption_keys.into_iter().collect();

        let verification_keys = verification_keys
            .into_iter()
            .map(AsPublicKeyRef::as_public_key)
            .collect();

        let session_key = pgp
            .new_decryptor()
            .with_decryption_key_refs(&decryption_keys)
            .decrypt_session_key(&packet)
            .map_err(|err| Error::CouldntDecryptKeyPacket { ty, err })?;

        Ok(Self {
            session_key,
            verification_keys,
        })
    }

    #[must_use]
    pub(crate) fn session_key(&self) -> &P::SessionKey {
        &self.session_key
    }

    pub fn decrypt(
        &self,
        pgp: &P,
        ics: EncryptedIcsRef,
        sig: Option<SignatureRef>,
    ) -> Result<DecryptedIcs>
    where
        P: PGPProviderSync,
    {
        let ics = BASE64_STANDARD
            .decode(ics.as_base64())
            .map_err(Error::CouldntDecodeIcs)?;

        let decryptor = {
            let decryptor = pgp.new_decryptor().with_session_key_ref(&self.session_key);

            if let Some(sig) = sig {
                decryptor
                    .with_verification_key_refs(&self.verification_keys)
                    .with_detached_signature_ref(
                        sig.as_armored().as_bytes(),
                        DetachedSignatureVariant::Plaintext,
                        true,
                    )
            } else {
                decryptor
            }
        };

        let ics = decryptor
            .decrypt(ics, DataEncoding::Bytes)
            .map_err(Error::CouldntDecryptIcs)?;

        if sig.is_some() {
            ics.verification_result().map_err(Error::CouldntVerifyIcs)?;
        }

        Ok(DecryptedIcs::from_bytes(ics.into_vec()))
    }
}
