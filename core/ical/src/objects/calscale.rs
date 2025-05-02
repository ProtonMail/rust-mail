/// Calendar scale.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.7.1>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalScale {
    #[default]
    Gregorian,
}
