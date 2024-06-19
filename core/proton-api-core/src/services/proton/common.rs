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
