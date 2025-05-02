/// RSVP expectation.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.17>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rsvp(bool);

impl Rsvp {
    #[must_use]
    pub fn yes() -> Self {
        Self::from(true)
    }

    #[must_use]
    pub fn no() -> Self {
        Self::from(false)
    }

    #[must_use]
    pub fn as_bool(&self) -> bool {
        self.0
    }
}

impl From<bool> for Rsvp {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
