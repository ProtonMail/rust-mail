use super::fido2;
use serde::{Deserialize, Serialize};

/// The 2FA status of a user.
///
/// This is used to determine if the user has 2FA enabled and what types are
/// enabled. If FIDO2 is enabled, the `fido` field will contain the enabled
/// FIDO2 keys and auth options.
///
/// TODO: Make `fido` a proper type.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TFA {
    /// 0 for disabled, 1 for OTP, 2 for FIDO2, 3 for bothJ
    enabled: u8,

    /// The FIDO2 keys and auth options, if FIDO2 is enabled.
    #[serde(rename = "FIDO2")]
    fido2: Option<fido2::Response>,
}

impl TFA {
    const TOTP: u8 = 1 << 0;
    const FIDO: u8 = 1 << 1;

    /// Returns `true` if either TOTP or FIDO2 is enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled != 0
    }

    /// Returns `true` if TOTP is enabled.
    #[must_use]
    pub fn totp_enabled(&self) -> bool {
        self.enabled & Self::TOTP != 0
    }

    /// Returns `true` if FIDO2 is enabled.
    #[must_use]
    pub fn fido_enabled(&self) -> bool {
        self.enabled & Self::FIDO != 0
    }

    /// Returns FIDO2 keys and auth options.
    #[must_use]
    pub fn fido_details(&self) -> Option<fido2::Response> {
        self.fido2.as_ref().filter(|_| self.fido_enabled()).cloned()
    }
}

/// `POST /auth/v4/2fa`
///
/// Authenticates a user using 2FA.
///
/// It is assumed the user already has a valid auth;
/// this request is only used to acquire additional auth scopes.
#[derive(Debug, Serialize, Deserialize)]
pub enum Post {
    /// Provides a TOTP code for 2FA.
    #[serde(rename = "TwoFactorCode")]
    TOTP(String),

    /// Provides a FIDO2 assertion for 2FA.
    #[serde(rename = "FIDO2")]
    FIDO(Box<fido2::Request>),
}

/// The response from a `POST /auth/v4/2fa` request.
///
/// Contains the additional auth scopes granted by the successful 2FA.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostRes {
    /// The granted auth scopes.
    pub scopes: Vec<String>,
}
