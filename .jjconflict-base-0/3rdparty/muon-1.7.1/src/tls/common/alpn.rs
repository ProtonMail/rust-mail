use crate::util::ByteSliceExt;
use derive_more::{AsRef, Deref, Display, From, Into};
use std::borrow::Borrow;
use std::fmt::Debug;

/// An ALPN protocol.
#[derive(Debug, Display, From, Into, Clone, Copy, PartialEq, Eq, Hash)]
#[display("{}", self.0.as_utf8_lossy())]
pub struct Alpn(&'static [u8]);

impl Alpn {
    /// Create a new ALPN protocol from the given bytes.
    #[must_use]
    pub const fn new(alpn: &'static [u8]) -> Self {
        Self(alpn)
    }

    /// Create a new ALPN protocol from the given string.
    #[must_use]
    pub const fn new_str(alpn: &'static str) -> Self {
        Self(alpn.as_bytes())
    }
}

impl AsRef<[u8]> for Alpn {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl Deref for Alpn {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl Borrow<[u8]> for Alpn {
    fn borrow(&self) -> &[u8] {
        self.0
    }
}
