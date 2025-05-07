use super::*;

/// Date-time created.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.7.1>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Created {
    pub value: DateTime,
}

impl Created {
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<CreatedViolation> {
        self.value.validate(cal).into_iter().map_into().collect()
    }
}

impl<T> From<T> for Created
where
    T: Into<DateTime>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for Created {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl IcsWrite<Property> for Created {
    fn write(&self, w: &mut IcsWriter) {
        self.value.write(w);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CreatedViolation {
    #[error("{0}")]
    InvalidValue(#[from] DateTimeViolation),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":20180101T120000")]
    #[test_case(":20180101T120000Z")]
    fn smoke(s: &str) {
        assert_trip!(s, Created as Property);
    }
}
