use super::*;

/// Time zone offset to.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct TzOffsetTo {
    pub value: UtcOffset,
}

impl From<UtcOffset> for TzOffsetTo {
    fn from(value: UtcOffset) -> Self {
        Self { value }
    }
}

impl IcsRead<Property> for TzOffsetTo {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self { value: r.value()? })
    }
}

impl IcsWrite<Property> for TzOffsetTo {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.value(self.value);
    }
}
