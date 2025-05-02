/// Time transparency.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.2.7>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transp {
    #[default]
    Opaque,
    Transparent,
}
