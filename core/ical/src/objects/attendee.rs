use super::*;

/// Attendee.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.4.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attendee {
    pub address: CalAddress,
    pub cn: Option<Cn>,
    pub cutype: Option<CuType>,
    pub role: Option<Role>,
    pub rsvp: Option<Rsvp>,
}

impl Attendee {
    #[must_use]
    pub fn with_cn(mut self, cn: impl Into<Cn>) -> Self {
        self.cn = Some(cn.into());
        self
    }

    #[must_use]
    pub fn with_cutype(mut self, cutype: CuType) -> Self {
        self.cutype = Some(cutype);
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub fn with_rsvp(mut self, rsvp: impl Into<Rsvp>) -> Self {
        self.rsvp = Some(rsvp.into());
        self
    }
}

impl<T> From<T> for Attendee
where
    T: Into<CalAddress>,
{
    fn from(address: T) -> Self {
        Self {
            address: address.into(),
            cn: None,
            cutype: None,
            role: None,
            rsvp: None,
        }
    }
}
