use super::*;

/// Date-time stamp.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.7.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtStamp {
    pub value: DateTime,
}

impl From<DateTime> for DtStamp {
    fn from(value: DateTime) -> Self {
        Self { value }
    }
}

impl Read<Property> for DtStamp {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl Write<Property> for DtStamp {
    fn write(&self, w: &mut Writer) {
        self.value.write(w);
    }
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
