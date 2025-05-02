/// Calendar user type.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.3>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CuType {
    #[default]
    Individual,
    Group,
    Resource,
    Room,
    Unknown,
}
