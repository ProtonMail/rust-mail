use crate::{CalendarEventDecryptor, Error, KeyPacket, KeyPacketRef, Result, UnlockedCalendarKey};
use proton_crypto::crypto::{Encryptor, EncryptorSync, PGPProviderSync};
use proton_crypto_account::keys::UnlockedAddressKeys;

pub struct CalendarKeyPacketUpgrader;

impl CalendarKeyPacketUpgrader {
    /// Upgrades `AddressKeyPacket` to `SharedKeyPacket`, re-encrypting the
    /// event from address key to calendar key.
    ///
    /// This is called whenever user replies to a Proton-to-Proton for the first
    /// time - those invites start encrypted using address key and ultimately we
    /// want for all events to be encrypted using calendar key.
    ///
    /// Note that we don't literally re-encrypt all of the fields, we just
    /// switch the session key so that *it* is encrypted using calendar key -
    /// the data remains the same, we just change how the key is represented.
    pub fn upgrade<P>(
        pgp: &P,
        address_keys: &UnlockedAddressKeys<P>,
        calendar_key: &UnlockedCalendarKey<P>,
        address_key_packet: KeyPacketRef,
    ) -> Result<KeyPacket>
    where
        P: PGPProviderSync,
    {
        let decryptor = CalendarEventDecryptor::for_address(pgp, address_keys, address_key_packet)?;

        let key_packet = pgp
            .new_encryptor()
            .with_encryption_key(&calendar_key.public_key)
            .encrypt_session_key(decryptor.session_key())
            .map_err(Error::CouldntEncryptSessionKey)?;

        Ok(KeyPacket::from_bytes(&key_packet))
    }
}
