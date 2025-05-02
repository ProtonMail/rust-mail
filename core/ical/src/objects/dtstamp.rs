use super::*;

/// Date-time stamp.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.7.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtStamp {
    pub value: DateTime,
}

impl From<DateTime> for DtStamp {
    fn from(value: DateTime) -> Self {
        Self { value }
    }
}
