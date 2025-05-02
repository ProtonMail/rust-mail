/// Status.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.11>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Tentative,
    Confirmed,
    Cancelled,
}
