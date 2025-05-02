use super::*;

/// Date-time stamp.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.7.2>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct DtStamp {
    pub value: DateTime,
}

impl DtStamp {
    pub(crate) fn validate(&self, cal: &VCalendar) -> Vec<DtStampViolation> {
        self.value.validate(cal).into_iter().map_into().collect()
    }
}

impl From<DateTime> for DtStamp {
    fn from(value: DateTime) -> Self {
        Self { value }
    }
}

impl IcsRead<Property> for DtStamp {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl IcsWrite<Property> for DtStamp {
    fn write(&self, w: &mut IcsWriter) {
        self.value.write(w);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DtStampViolation {
    #[error("{0}")]
    InvalidValue(#[from] DateTimeViolation),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":20180101T120000Z")]
    fn smoke(s: &str) {
        assert_trip!(s, DtStamp as Property);
    }
}
