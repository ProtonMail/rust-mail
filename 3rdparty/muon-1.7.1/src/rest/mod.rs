//! ## Rest
//!
//! This module defines types for working with the Proton REST API.

use crate::Result;
use derive_more::From;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `/auth`
pub mod auth;

/// `/core`
pub mod core;

/// `/tests`
pub mod tests;

/// The API's error type.
#[derive(Debug, From, Serialize, Deserialize)]
pub struct ApiErr {
    /// The error code.
    pub code: u16,

    /// The error message.
    pub error: String,
}

/// The API's "bool" type (which is actually an integer).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bool {
    /// The "false" value.
    #[default]
    False = 0,

    /// The "true" value.
    True = 1,
}

impl Serialize for Bool {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (*self as i32).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Bool {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match i32::deserialize(d)? {
            0 => Ok(Self::False),
            1 => Ok(Self::True),
            v => Err(serde::de::Error::custom(format!("invalid bool value: {v}"))),
        }
    }
}

impl From<bool> for Bool {
    fn from(b: bool) -> Self {
        if b {
            Self::True
        } else {
            Self::False
        }
    }
}

impl From<Bool> for bool {
    fn from(b: Bool) -> Self {
        match b {
            Bool::False => false,
            Bool::True => true,
        }
    }
}
