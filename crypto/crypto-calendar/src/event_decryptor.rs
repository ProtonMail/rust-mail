use crate::{
    DecryptedIcs, EncryptedIcsRef, Error, KeyPacketRef, KeyPackets, Result, SignatureRef,
    UnlockedCalendarKey,
};
use anyhow::{anyhow, Context};
use base64::{prelude::BASE64_STANDARD, Engine};
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
    session_key: P::SessionKey,
    verification_keys: Vec<&'a P::PublicKey>,
}

impl<'a, P> CalendarEventDecryptor<'a, P>
where
    P: PGPProviderSync,
{
    pub fn new(
        pgp: &'a P,
        address_keys: &'a UnlockedAddressKeys<P>,
        calendar_key: &UnlockedCalendarKey<P>,
        key_packets: KeyPackets<KeyPacketRef>,
    ) -> Result<Self> {
        if let Some(packet) = key_packets.address_key_packet {
            Self::new_ex(
                pgp,
                packet,
                address_keys.iter(),
                address_keys.iter(),
                "address key packet",
            )
        } else if let Some(packet) = key_packets.shared_key_packet {
            Self::new_ex(
                pgp,
                packet,
                iter::once(calendar_key),
                address_keys.iter(),
                "shared key packet",
            )
        } else {
            Err(Error::from(anyhow!(
                "Both address key packet and shared key packet are missing"
            )))
        }
    }

    fn new_ex<'b, D, V>(
        pgp: &'a P,
        packet: KeyPacketRef,
        decryption_keys: impl IntoIterator<Item = &'b D>,
        verification_keys: impl IntoIterator<Item = &'a V>,
        ty: &str,
    ) -> Result<Self>
    where
        D: AsRef<P::PrivateKey> + 'b,
        V: AsPublicKeyRef<P::PublicKey> + 'a,
    {
        let packet = BASE64_STANDARD
            .decode(packet.as_base64())
            .with_context(|| format!("Couldn't decode {ty}"))?;

        let decryption_keys: Vec<_> = decryption_keys.into_iter().collect();

        let verification_keys = verification_keys
            .into_iter()
            .map(AsPublicKeyRef::as_public_key)
            .collect();

        let session_key = pgp
            .new_decryptor()
            .with_decryption_key_refs(&decryption_keys)
            .decrypt_session_key(&packet)
            .with_context(|| format!("Couldn't decrypt {ty}"))?;

        Ok(Self {
            session_key,
            verification_keys,
        })
    }

    pub fn decrypt(
        &self,
        pgp: &P,
        ics: EncryptedIcsRef,
        sign: Option<SignatureRef>,
    ) -> Result<DecryptedIcs>
    where
        P: PGPProviderSync,
    {
        let ics = BASE64_STANDARD
            .decode(ics.as_base64())
            .context("Couldn't decode *.ics")?;

        let decryptor = {
            let decryptor = pgp.new_decryptor().with_session_key_ref(&self.session_key);

            if let Some(sign) = sign {
                decryptor
                    .with_verification_key_refs(&self.verification_keys)
                    .with_detached_signature_ref(
                        sign.as_armored().as_bytes(),
                        DetachedSignatureVariant::Plaintext,
                        true,
                    )
            } else {
                decryptor
            }
        };

        let ics = decryptor
            .decrypt(ics, DataEncoding::Bytes)
            .context("Couldn't decrypt *.ics")?;

        if sign.is_some() {
            ics.verification_result().context("Couldn't verify *.ics")?;
        }

        Ok(DecryptedIcs::from_bytes(ics.into_vec()))
    }
}
