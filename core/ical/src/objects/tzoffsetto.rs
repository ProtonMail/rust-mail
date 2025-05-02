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
