use super::*;

/// Time zone offset to.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TzOffsetTo {
    pub value: UtcOffset,
}

impl From<UtcOffset> for TzOffsetTo {
    fn from(value: UtcOffset) -> Self {
        Self { value }
    }
}

impl Read<Property> for TzOffsetTo {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self { value: r.value()? })
    }
}

impl Write<Property> for TzOffsetTo {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(self.value);
    }
}
