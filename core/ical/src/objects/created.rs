use super::*;

/// Date-time created.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.7.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    pub value: DateTime,
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

impl Read<Property> for Created {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.prop()? })
    }
}

impl Write<Property> for Created {
    fn write(&self, w: &mut Writer) {
        self.value.write(w);
    }
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
