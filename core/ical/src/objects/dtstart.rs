use super::*;

/// Date-time start.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.2.4>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct DtStart {
    pub value: DateOrDt,
}

impl DtStart {
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<DtStartViolation> {
        self.value.validate(cal).into_iter().map_into().collect()
    }
}

impl<T> From<T> for DtStart
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for DtStart {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl IcsWrite<Property> for DtStart {
    fn write(&self, w: &mut IcsWriter) {
        self.value.write(w);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DtStartViolation {
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
        assert_trip!(s, DtStart as Property);
    }
}
