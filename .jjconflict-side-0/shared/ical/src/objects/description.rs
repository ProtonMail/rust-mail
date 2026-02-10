use super::*;

/// Description.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.5>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Description {
    pub value: Text,
}

impl<T> From<T> for Description
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for Description {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for Description {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":Some description")]
    #[test_case(":Some description\\, with funny chars!")]
    fn smoke(s: &str) {
        assert_trip!(s, Description as Property);
    }
}
