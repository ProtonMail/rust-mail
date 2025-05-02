use super::*;

/// Unique identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.4.7>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Uid {
    pub value: Text,
}

impl<T> From<T> for Uid
where
    T: Into<Text>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl Read<Property> for Uid {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self { value: r.value()? })
    }
}

impl Write<Property> for Uid {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(&self.value);
    }
}
