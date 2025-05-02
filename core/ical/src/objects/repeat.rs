/// Repeat count.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.6.2>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repeat {
    pub value: u32,
}

impl From<u32> for Repeat {
    fn from(value: u32) -> Self {
        Self { value }
    }
}
