use super::*;

/// Attendee.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.4.1>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
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

impl IcsRead<Property> for Attendee {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut cn = None;
        let mut cutype = None;
        let mut role = None;
        let mut rsvp = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "CN", &mut cn)
                || e.try_param(r, "CUTYPE", &mut cutype)
                || e.try_param(r, "ROLE", &mut role)
                || e.try_param(r, "RSVP", &mut rsvp)
            {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r);
        }

        Some(Self {
            address: r.value()?,
            cn,
            cutype,
            role,
            rsvp,
        })
    }
}

impl IcsWrite<Property> for Attendee {
    fn write(&self, w: &mut IcsWriter) {
        w.param_opt("CN", self.cn.as_ref());
        w.param_opt("CUTYPE", self.cutype.as_ref());
        w.param_opt("ROLE", self.role.as_ref());
        w.param_opt("RSVP", self.rsvp.as_ref());
        w.raw(":");
        w.value(&self.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ical, utils::*};
    use pretty_assertions as pa;
    use test_case::test_case;

    #[test]
    fn build() {
        let target = Attendee::from(email("someone@localhost"))
            .with_cn("Someone Somewhere")
            .with_cutype(CuType::Individual)
            .with_role(Role::Chair)
            .with_rsvp(true);

        let expected = ical! {"
            ;CN=Someone Somewhere;CUTYPE=INDIVIDUAL;ROLE=CHAIR;RSVP=TRUE:mailto:someone
             @localhost
        "};

        pa::assert_eq!(expected, target.to_string(Property));
    }

    #[test_case(":mailto:someone@localhost")]
    #[test_case(";CN=Someone:mailto:someone@localhost")]
    #[test_case(";CN=Someone Somewhere:mailto:someone@localhost")]
    #[test_case(";CUTYPE=INDIVIDUAL:mailto:someone@localhost")]
    #[test_case(";CUTYPE=GROUP:mailto:someone@localhost")]
    #[test_case(";ROLE=CHAIR:mailto:someone@localhost")]
    #[test_case(";ROLE=OPT-PARTICIPANT:mailto:someone@localhost")]
    #[test_case(";RSVP=TRUE:mailto:someone@localhost")]
    #[test_case(";RSVP=FALSE:mailto:someone@localhost")]
    #[test_case(";CUTYPE=ROOM;ROLE=CHAIR;RSVP=TRUE:mailto:someone@localhost")]
    fn smoke(s: &str) {
        assert_trip!(s, Attendee as Property);
    }
}
