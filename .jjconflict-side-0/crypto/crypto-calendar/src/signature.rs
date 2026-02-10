#[derive(Clone, Debug)]
pub struct Signature(String);

impl Signature {
    #[must_use]
    pub fn from_armored(sig: String) -> Self {
        Self(sig)
    }

    #[must_use]
    pub fn as_armored(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_armored(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_ref(&self) -> SignatureRef<'_> {
        SignatureRef::from_armored(self.as_armored())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SignatureRef<'a>(&'a str);

impl<'a> SignatureRef<'a> {
    #[must_use]
    pub fn from_armored(sig: &'a str) -> Self {
        Self(sig)
    }

    #[must_use]
    pub fn as_armored(&self) -> &'a str {
        self.0
    }
}
