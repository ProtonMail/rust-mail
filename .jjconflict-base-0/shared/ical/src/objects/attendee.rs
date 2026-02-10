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
    pub email: Option<Email>,
    pub role: Option<Role>,
    pub partstat: Option<PartStat>,
    pub rsvp: Option<Rsvp>,

    // TODO(NGC-36) hacky - X- params should be handled separately from the RFC ones
    pub x_pm_token: Option<ParamValue>,
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
    pub fn with_email(mut self, email: impl Into<Email>) -> Self {
        self.email = Some(email.into());
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub fn with_partstat(mut self, partstat: PartStat) -> Self {
        self.partstat = Some(partstat);
        self
    }

    #[must_use]
    pub fn with_rsvp(mut self, rsvp: impl Into<Rsvp>) -> Self {
        self.rsvp = Some(rsvp.into());
        self
    }

    #[must_use]
    pub fn with_x_pm_token(mut self, x_pm_token: impl Into<ParamValue>) -> Self {
        self.x_pm_token = Some(x_pm_token.into());
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
            email: None,
            role: None,
            partstat: None,
            rsvp: None,
            x_pm_token: None,
        }
    }
}

impl IcsRead<Property> for Attendee {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut cn = None;
        let mut cutype = None;
        let mut email = None;
        let mut role = None;
        let mut partstat = None;
        let mut rsvp = None;
        let mut x_pm_token = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "CN", &mut cn)
                || e.try_param(r, "CUTYPE", &mut cutype)
                || e.try_param(r, "EMAIL", &mut email)
                || e.try_param(r, "ROLE", &mut role)
                || e.try_param(r, "PARTSTAT", &mut partstat)
                || e.try_param(r, "RSVP", &mut rsvp)
                || e.try_param(r, "X-PM-TOKEN", &mut x_pm_token)
            {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r, Kind::Property)?;
        }

        Some(Self {
            address: r.value()?,
            cn,
            cutype,
            email,
            role,
            partstat,
            rsvp,
            x_pm_token,
        })
    }
}

impl IcsWrite<Property> for Attendee {
    fn write(&self, w: &mut IcsWriter) {
        w.param_opt("CN", self.cn.as_ref());
        w.param_opt("CUTYPE", self.cutype.as_ref());
        w.param_opt("EMAIL", self.email.as_ref());
        w.param_opt("ROLE", self.role.as_ref());
        w.param_opt("PARTSTAT", self.partstat.as_ref());
        w.param_opt("RSVP", self.rsvp.as_ref());
        w.param_opt("X-PM-TOKEN", self.x_pm_token.as_ref());
        w.raw(":");
        w.value(&self.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ics, utils::*};
    use pretty_assertions as pa;
    use test_case::test_case;

    #[test]
    fn build() {
        let target = Attendee::from(email("someone@localhost"))
            .with_cn("Someone Somewhere")
            .with_cutype(CuType::Individual)
            .with_role(Role::Chair)
            .with_rsvp(true);

        let expected = ics! {"
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
    #[test_case(";EMAIL=someone@somewhere.com:mailto:localhost")]
    #[test_case(";ROLE=CHAIR:mailto:someone@localhost")]
    #[test_case(";ROLE=OPT-PARTICIPANT:mailto:someone@localhost")]
    #[test_case(";PARTSTAT=ACCEPTED:mailto:someone@localhost")]
    #[test_case(";PARTSTAT=TENTATIVE:mailto:someone@localhost")]
    #[test_case(";RSVP=TRUE:mailto:someone@localhost")]
    #[test_case(";RSVP=FALSE:mailto:someone@localhost")]
    #[test_case(";CUTYPE=ROOM;ROLE=CHAIR;RSVP=TRUE:mailto:someone@localhost")]
    #[test_case(";X-PM-TOKEN=dc5d4a72:mailto:someone@localhost")]
    fn smoke(s: &str) {
        assert_trip!(s, Attendee as Property);
    }

    #[test]
    fn without_mailto() {
        // Technically illegal, but some clients really do forget to generate
        // the `mailto:` bit
        assert_trip!(
            ":someone@localhost.com" => ":mailto:someone@localhost.com",
            yielding [
                ReadMsg {
                    at: Some(Span::new((1, 2), (1, 23))),
                    body: "quirky email address (missing `mailto:`)".into(),
                    kind: ReadMsgKind::Warning,
                    context: vec![
                        Spanned::new(Span::new((1, 2), (1, 2)), "`CalAddress`".into()),
                        Spanned::new(Span::new((1, 2), (1, 2)), "`EmailAddress`".into()),
                    ],
                },
            ],
            Attendee as Property
        );
    }
}
