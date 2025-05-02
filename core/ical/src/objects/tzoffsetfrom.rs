use super::*;

/// Time zone offset from.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.3>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TzOffsetFrom {
    pub value: UtcOffset,
}

impl From<UtcOffset> for TzOffsetFrom {
    fn from(value: UtcOffset) -> Self {
        Self { value }
    }
}
