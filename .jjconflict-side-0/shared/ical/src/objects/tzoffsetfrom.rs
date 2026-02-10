use super::*;

/// Time zone offset from.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.3>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct TzOffsetFrom {
    pub value: UtcOffset,
}

impl From<UtcOffset> for TzOffsetFrom {
    fn from(value: UtcOffset) -> Self {
        Self { value }
    }
}

impl IcsRead<Property> for TzOffsetFrom {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for TzOffsetFrom {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(self.value);
    }
}
