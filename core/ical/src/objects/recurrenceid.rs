use super::*;

/// Recurrence id.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.1>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct RecurrenceId {
    pub value: DateOrDt,
}

impl RecurrenceId {
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<RecurrenceIdViolation> {
        self.value.validate(cal).into_iter().map_into().collect()
    }
}

impl<T> From<T> for RecurrenceId
where
    T: Into<DateOrDt>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for RecurrenceId {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl IcsWrite<Property> for RecurrenceId {
    fn write(&self, w: &mut IcsWriter) {
        self.value.write(w);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RecurrenceIdViolation {
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
        assert_trip!(s, RecurrenceId as Property);
    }
}
