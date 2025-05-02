use super::*;

/// Date-time end.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct DtEnd {
    pub value: DateOrDt,
}

impl DtEnd {
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<DtEndViolation> {
        self.value.validate(cal).into_iter().map_into().collect()
    }
}

impl<T> From<T> for DtEnd
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for DtEnd {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl IcsWrite<Property> for DtEnd {
    fn write(&self, w: &mut IcsWriter) {
        self.value.write(w);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DtEndViolation {
    #[error("{0}")]
    InvalidValue(#[from] DateTimeViolation),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(";VALUE=DATE:20180101")]
    #[test_case(":20180101T120000Z")]
    fn smoke(s: &str) {
        assert_trip!(s, DtEnd as Property);
    }
}
