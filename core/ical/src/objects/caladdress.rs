use super::*;

/// Calendar address.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.3.3>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalAddress {
    Email(EmailAddress),
    Url(UrlAddress),
}

impl From<EmailAddress> for CalAddress {
    fn from(value: EmailAddress) -> Self {
        CalAddress::Email(value)
    }
}

impl From<UrlAddress> for CalAddress {
    fn from(value: UrlAddress) -> Self {
        CalAddress::Url(value)
    }
}

/// An email address; see [`CalAddress`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmailAddress {
    value: Text,
}

impl EmailAddress {
    #[must_use]
    pub fn value(&self) -> &Text {
        &self.value
    }

    #[must_use]
    pub fn into_value(self) -> Text {
        self.value
    }
}

impl<T> From<T> for EmailAddress
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

/// An URL address; see [`CalAddress`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlAddress {
    value: Text,
}

impl UrlAddress {
    #[must_use]
    pub fn value(&self) -> &Text {
        &self.value
    }

    #[must_use]
    pub fn into_value(self) -> Text {
        self.value
    }
}

impl<T> From<T> for UrlAddress
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
