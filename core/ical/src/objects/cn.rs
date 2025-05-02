use super::*;

/// Common name.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cn {
    pub value: ParamValue,
}

impl<T> From<T> for Cn
where
    T: Into<ParamValue>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl Read<Value> for Cn {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { value: r.value()? })
    }
}

impl Write<Value> for Cn {
    fn write(&self, w: &mut Writer) {
        self.value.write(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("John Smith")]
    #[test_case("\"Adam, Eve\"")]
    fn smoke(s: &str) {
        assert_trip!(s, Cn as Value);
    }
}
