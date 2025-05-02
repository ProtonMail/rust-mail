use super::*;

/// Organizer.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.4.3>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Organizer {
    pub address: CalAddress,
    pub cn: Option<Cn>,
}

impl Organizer {
    #[must_use]
    pub fn with_cn(mut self, cn: impl Into<Cn>) -> Self {
        self.cn = Some(cn.into());
        self
    }
}

impl<T> From<T> for Organizer
where
    T: Into<CalAddress>,
{
    fn from(address: T) -> Self {
        Self {
            address: address.into(),
            cn: None,
        }
    }
}
