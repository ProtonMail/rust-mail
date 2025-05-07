use super::*;

/// Summary.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.12>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Summary {
    pub value: Text,
}

impl<T> From<T> for Summary
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for Summary {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for Summary {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}
