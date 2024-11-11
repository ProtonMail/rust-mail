//! Common types used by the Proton API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!

use core::fmt;
use secrecy::{CloneableSecret, DebugSecret, Zeroize};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::ops::Deref;

//  ENUMS
//==============================================================================

/// Human verification type returned by the API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum HumanVerificationType {
    /// User needs to solve a Captcha, use [`crate::captcha_get`] to retrieve the token, solve in a web
    /// browser/view and retrieve the token posted via an `HVCaptchaMessage`.
    Captcha,

    /// User needs to verify via a token send via an email. Note: Request for this
    /// verification is not yet implemented.
    Email,

    /// User needs to verify via a token send via sms. Note: Request for this verification is not
    /// yet inmplemented.
    Sms,
}

impl HumanVerificationType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Captcha => "captcha",
            Self::Email => "email",
            Self::Sms => "sms",
        }
    }
}

/// The theme being used in Images Logo.
#[derive(Clone, Copy, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LightOrDarkMode {
    /// Light mode
    Light,

    /// Dark mode
    Dark,
}

//  STRUCTS
//==============================================================================

/// Remote ID.
///
/// This minimal struct is simply a wrapper around a [`String`], and is used to
/// formalise all IDs used by the Proton API.
///
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RemoteId(String);

impl RemoteId {
    /// Create a new [`RemoteId`] from a [`String`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Convert the [`RemoteId`] into the inner [`String`].
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl CloneableSecret for RemoteId {}

impl DebugSecret for RemoteId {}

impl Deref for RemoteId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for RemoteId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RemoteId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for RemoteId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl Zeroize for RemoteId {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
