use super::*;

/// Product identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.7.3>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct ProdId {
    pub value: Text,
}

impl<T> From<T> for ProdId
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self {
            value: Text::new(value),
        }
    }
}

impl IcsRead<Property> for ProdId {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }

    fn reasonable_default() -> Option<Self> {
        Some(Self::from("UNKNOWN"))
    }
}

impl IcsWrite<Property> for ProdId {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}
