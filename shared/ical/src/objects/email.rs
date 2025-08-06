use super::*;

/// Email address; a non-standard field used by Apple:
///
/// ```text
/// ATTENDEE;CN=Jan Kowalski;EMAIL=jan@kowalski.com:mailto:RANDOMTOKEN@imip.me.com
/// ORGANIZER;CN=Jan Kowalski;EMAIL=jan@kowalski.com:mailto:RANDOMTOKEN@imip.me.com
/// ```
///
/// Note that as compared to [`EmailAddress`] or even [`SentBy`], this [`Email`]
/// doesn't use the `mailto:` prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Email {
    pub value: EmailAddress,
}

impl<T> From<T> for Email
where
    T: Into<EmailAddress>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Value> for Email {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self {
            value: EmailAddress::from(r.value::<ParamValue>()?.into_string()),
        })
    }
}

impl IcsWrite<Value> for Email {
    fn write(&self, w: &mut IcsWriter) {
        self.value.value().write(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("someone@localhost")]
    fn smoke(s: &str) {
        assert_trip!(s, Email as Value);
    }
}
