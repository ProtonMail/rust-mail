use super::*;

/// Time zone name.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.2>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct TzName {
    pub value: Text,
}

impl<T> From<T> for TzName
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl IcsRead<Property> for TzName {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for TzName {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(&self.value);
    }
}
