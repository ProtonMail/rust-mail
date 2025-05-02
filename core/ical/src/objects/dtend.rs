use super::*;

/// Date-time end.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtEnd {
    pub value: DateOrDt,
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

impl Read<Property> for DtEnd {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl Write<Property> for DtEnd {
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
        assert_trip!(s, DtEnd as Property);
    }
}
