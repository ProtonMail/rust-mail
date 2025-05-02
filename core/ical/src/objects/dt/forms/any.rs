use super::*;

/// Interpretation of [`DateTime`]'s time component.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnyForm {
    Local,
    Utc,
    Tz(TzId),
}
