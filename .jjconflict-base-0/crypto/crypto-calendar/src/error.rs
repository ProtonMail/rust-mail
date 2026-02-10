use proton_crypto::{CryptoError, crypto::VerificationError};
use std::result;
use thiserror::Error;

pub type Result<T, E = Error> = result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Couldn't decode *.ics: {0}")]
    CouldntDecodeIcs(base64::DecodeError),

    #[error("Couldn't encrypt *.ics: {0}")]
    CouldntEncryptIcs(CryptoError),

    #[error("Couldn't decrypt *.ics: {0}")]
    CouldntDecryptIcs(CryptoError),

    #[error("Couldn't sign *.ics: {0}")]
    CouldntSignIcs(CryptoError),

    #[error("Couldn't verify *.ics: {0}")]
    CouldntVerifyIcs(VerificationError),

    #[error("Couldn't decode {ty} key packet: {err}")]
    CouldntDecodeKeyPacket {
        ty: &'static str,
        err: base64::DecodeError,
    },

    #[error("Couldn't decrypt {ty} key packet: {err}")]
    CouldntDecryptKeyPacket { ty: &'static str, err: CryptoError },

    #[error("Both address key packet and shared key packet are missing")]
    BothKeyPacketsAreMissing,

    #[error("Couldn't find primary address key")]
    CouldntFindPrimaryAddressKey,

    #[error("Couldn't find primary calendar key")]
    CouldntFindPrimaryCalendarKey,

    #[error("Couldn't find passphrase for calendar member {0}")]
    CouldntFindCalendarPassphrase(String),

    #[error("Couldn't generate session key: {0}")]
    CouldntGenerateSessionKey(CryptoError),

    #[error("Couldn't encrypt session key: {0}")]
    CouldntEncryptSessionKey(CryptoError),

    #[error("Couldn't generate calendar key: {0}")]
    CouldntGenerateCalendarKey(CryptoError),

    #[error("Couldn't import calendar private key: {0}")]
    CouldntImportCalendarPrivateKey(CryptoError),

    #[error("Couldn't export calendar private key: {0}")]
    CouldntExportCalendarPrivateKey(CryptoError),

    #[error("Couldn't encrypt calendar passphrase: {0}")]
    CouldntEncryptCalendarPassphrase(CryptoError),

    #[error("Couldn't decrypt calendar passphrase: {0}")]
    CouldntDecryptCalendarPassphrase(CryptoError),

    #[error("Couldn't armor calendar passphrase: {0}")]
    CouldntArmorCalendarPassphrase(CryptoError),

    #[error("Couldn't sign calendar passphrase: {0}")]
    CouldntSignCalendarPassphrase(CryptoError),

    #[error("Couldn't verify calendar passphrase: {0}")]
    CouldntVerifyCalendarPassphrase(VerificationError),

    #[error("Couldn't convert calendar's private key into public key: {0}")]
    CouldntConvertCalendarPrivateKeyToPublic(CryptoError),
}
