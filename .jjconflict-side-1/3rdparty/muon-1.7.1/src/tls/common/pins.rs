use crate::common::prelude::*;
use crate::env::{DynEnv, Env};
use crate::tls::{TlsCert, Verifier, VerifyRes};
use crate::util::{ByteSliceErr, ByteSliceExt, IntoIterExt};
use crate::Result;
use derive_more::{AsRef, Deref, Display, FromStr};
use std::borrow::Borrow;
use std::collections::hash_set::{IntoIter as HashSetIntoIter, Iter as HashSetIter};
use std::collections::HashSet;
use std::fmt::{Formatter, Result as FmtResult};

/// A TLS pin.
///
/// This is the SHA-256 hash of a certificate's Subject Public Key Info (SPKI).
#[derive(Debug, AsRef, Deref, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TlsPin([u8; 32]);

impl TlsPin {
    /// Create a new TLS pin from the given SHA-256 hash.
    #[must_use]
    pub fn new(pin: [u8; 32]) -> Self {
        Self(pin)
    }
}

impl FromStr for TlsPin {
    type Err = ByteSliceErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.b64_into()?))
    }
}

impl Borrow<[u8; 32]> for TlsPin {
    fn borrow(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for TlsPin {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "{}", self.as_b64())
    }
}

/// A TLS pin set.
///
/// This type wraps a set of TLS pins, providing convenient methods for checking
/// whether a given certificate matches any of the pins in the set.
#[derive(Debug, Default, Clone)]
pub struct TlsPinSet(HashSet<TlsPin>);

impl TlsPinSet {
    /// Create a new TLS pin set from the given pins.
    #[must_use]
    pub fn new(pins: impl IntoIterator<Item = TlsPin>) -> Self {
        Self(pins.into_iter().collect())
    }

    /// Create a new TLS pin set from the base64-encoded pins.
    pub fn from_b64<I, T>(pins: I) -> Result<Self, ByteSliceErr>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let pins = (pins.into_iter())
            .map(|pin| pin.as_ref().parse())
            .try_into_set()?;

        Ok(Self::new(pins))
    }

    /// Check whether the given certificate is pinned.
    #[must_use]
    pub fn contains(&self, cert: &TlsCert) -> bool {
        self.0.contains(&cert.public_key().raw.sha256())
    }

    /// Check whether any of the given certificates are pinned.
    #[must_use]
    pub fn contains_any(&self, certs: &[TlsCert]) -> bool {
        certs.iter().any(|cert| self.contains(cert))
    }

    /// Check whether all of the given certificates are pinned.
    #[must_use]
    pub fn contains_all(&self, certs: &[TlsCert]) -> bool {
        certs.iter().all(|cert| self.contains(cert))
    }
}

impl IntoIterator for TlsPinSet {
    type Item = TlsPin;
    type IntoIter = HashSetIntoIter<TlsPin>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a TlsPinSet {
    type Item = &'a TlsPin;
    type IntoIter = HashSetIter<'a, TlsPin>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A verifier that checks a chain of certificates against a set of pins.
#[derive(Debug)]
pub struct TlsPinVerifier(DynEnv);

impl TlsPinVerifier {
    /// Create a new TLS pin verifier from the given map.
    #[must_use]
    pub fn new(env: impl IntoDyn<DynEnv>) -> Self {
        Self(env.into_dyn())
    }
}

impl Verifier for TlsPinVerifier {
    fn verify(&self, host: &Host, head: &TlsCert, _: &[TlsCert]) -> Result<VerifyRes> {
        let Some(pins) = self.0.pins(host) else {
            trace!(%host, "no pins for host, not verifying");
            return Ok(VerifyRes::Delegate);
        };

        if pins.contains(head) {
            trace!(%host, "leaf certificate is pinned");
            return Ok(VerifyRes::Accept);
        }

        Ok(VerifyRes::Reject)
    }
}

if_sealed! {
    impl crate::Sealed for TlsPinVerifier {}
}
