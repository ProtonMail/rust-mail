use super::*;

/// Location.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.7>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Location {
    pub value: Text,
}

impl<T> From<T> for Location
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for Location {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for Location {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":Some location")]
    #[test_case(":https://somewhere.com")]
    fn smoke(s: &str) {
        assert_trip!(s, Location as Property);
    }
}
