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

impl IcsRead<Value> for CalAddress {
    fn read(r: &mut IcsReader) -> Option<Self> {
        if let Some(email) = r.attempt(IcsReader::value) {
            Some(CalAddress::Email(email))
        } else {
            Some(CalAddress::Url(r.value()?))
        }
    }
}

impl IcsWrite<Value> for CalAddress {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            CalAddress::Email(this) => w.value(this),
            CalAddress::Url(this) => w.value(this),
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, ZvalConvert)]
    struct PhpCalAddress {
        kind: String,
        value: String,
    }

    impl From<CalAddress> for PhpCalAddress {
        fn from(value: CalAddress) -> Self {
            match value {
                CalAddress::Email(value) => Self {
                    kind: "Email".into(),
                    value: value.value.into_string(),
                },
                CalAddress::Url(value) => Self {
                    kind: "Url".into(),
                    value: value.value.into_string(),
                },
            }
        }
    }

    impl TryFrom<PhpCalAddress> for CalAddress {
        type Error = ();

        fn try_from(value: PhpCalAddress) -> Result<Self, Self::Error> {
            match value.kind.as_str() {
                "Email" => Ok(Self::Email(EmailAddress::from(value.value))),
                "Url" => Ok(Self::Url(UrlAddress::from(value.value))),
                _ => Err(()),
            }
        }
    }

    impl<'a> FromPhpZval<'a> for CalAddress {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            PhpCalAddress::from_zval(zval)?.try_into().ok()
        }
    }

    impl IntoPhpZval for CalAddress {
        const TYPE: PhpDataType = PhpDataType::String;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            zval.set_string(&self.to_string(Value), persistent)
        }
    }
}

/// An email address; see [`CalAddress`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Value> for EmailAddress {
    fn read(r: &mut IcsReader) -> Option<Self> {
        if r.try_string("mailto:").is_some() {
            Some(Self { value: r.value()? })
        } else {
            None
        }
    }
}

impl IcsWrite<Value> for EmailAddress {
    fn write(&self, w: &mut IcsWriter) {
        w.raw("mailto:");
        w.value(&self.value);
    }
}

impl IcsRead<Property> for EmailAddress {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;
        r.value()
    }
}

impl IcsWrite<Property> for EmailAddress {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(self);
    }
}

/// An URL address; see [`CalAddress`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Value> for UrlAddress {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Value> for UrlAddress {
    fn write(&self, w: &mut IcsWriter) {
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
