use base64::{Engine, prelude::BASE64_STANDARD};

#[derive(Clone, Debug)]
pub struct EncryptedIcs(String);

impl EncryptedIcs {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_base64(BASE64_STANDARD.encode(bytes))
    }

    #[must_use]
    pub fn from_base64(ics: String) -> Self {
        Self(ics)
    }

    #[must_use]
    pub fn as_base64(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_base64(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_ref(&self) -> EncryptedIcsRef<'_> {
        EncryptedIcsRef::from_base64(self.as_base64())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncryptedIcsRef<'a>(&'a str);

impl<'a> EncryptedIcsRef<'a> {
    #[must_use]
    pub fn from_base64(ics: &'a str) -> Self {
        Self(ics)
    }

    #[must_use]
    pub fn as_base64(&self) -> &'a str {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct DecryptedIcs(Vec<u8>);

impl DecryptedIcs {
    #[must_use]
    pub fn from_bytes(ics: Vec<u8>) -> Self {
        Self(ics)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}
