use super::*;

/// Common name.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Value> for Cn {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Value> for Cn {
    fn write(&self, w: &mut IcsWriter) {
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
