use super::*;

/// Sequence number.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.7.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Sequence {
    pub value: u32,
}

impl From<u32> for Sequence {
    fn from(value: u32) -> Self {
        Self { value }
    }
}

impl Read<Property> for Sequence {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self { value: r.value()? })
    }
}

impl Write<Property> for Sequence {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(self.value);
    }
}
