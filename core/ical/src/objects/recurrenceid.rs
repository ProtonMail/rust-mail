use super::*;

/// Recurrence id.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.6.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurrenceId {
    pub value: DateOrDt,
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

impl Read<Property> for RecurrenceId {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl Write<Property> for RecurrenceId {
    fn write(&self, w: &mut Writer) {
        self.value.write(w);
    }
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
