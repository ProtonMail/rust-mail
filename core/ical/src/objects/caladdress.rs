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

impl Read<Value> for CalAddress {
    fn read(r: &mut Reader) -> Option<Self> {
        if let Some(email) = r.attempt(Reader::value) {
            Some(CalAddress::Email(email))
        } else {
            Some(CalAddress::Url(r.value()?))
        }
    }
}

impl Write<Value> for CalAddress {
    fn write(&self, w: &mut Writer) {
        match self {
            CalAddress::Email(this) => w.value(this),
            CalAddress::Url(this) => w.value(this),
        }
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

impl Read<Value> for EmailAddress {
    fn read(r: &mut Reader) -> Option<Self> {
        if r.try_string("mailto:").is_some() {
            Some(Self { value: r.value()? })
        } else {
            None
        }
    }
}

impl Write<Value> for EmailAddress {
    fn write(&self, w: &mut Writer) {
        w.raw("mailto:");
        w.value(&self.value);
    }
}

impl Read<Property> for EmailAddress {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;
        r.value()
    }
}

impl Write<Property> for EmailAddress {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(self);
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

impl Read<Value> for UrlAddress {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.value()? })
    }
}

impl Write<Value> for UrlAddress {
    fn write(&self, w: &mut Writer) {
        w.value(&self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email() {
        let target = CalAddress::from(EmailAddress::from("someone@somewhere.com"));

        assert_eq!("mailto:someone@somewhere.com", target.to_string(Value));

        assert_eq!(
            target,
            CalAddress::from_str("mailto:someone@somewhere.com", Value).unwrap()
        );
    }

    #[test]
    fn url() {
        let target = CalAddress::from(UrlAddress::from("https://proton.me"));

        assert_eq!("https://proton.me", target.to_string(Value));

        assert_eq!(
            target,
            CalAddress::from_str("https://proton.me", Value).unwrap()
        );
    }
}
