/// Interpretation of [`DateTime`]'s time component; subset of [`AnyForm`].
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UtcOrLocalForm {
    Local,
    Utc,
}
