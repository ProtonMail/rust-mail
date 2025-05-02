/// Classification.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.1.3>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Class {
    #[default]
    Public,
    Private,
    Confidential,
}
