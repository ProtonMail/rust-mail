/// Sequence number.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.7.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub value: u32,
}

impl From<u32> for Sequence {
    fn from(value: u32) -> Self {
        Self { value }
    }
}
