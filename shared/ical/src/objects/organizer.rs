use super::*;

/// Organizer.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.4.3>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Organizer {
    pub address: CalAddress,
    pub cn: Option<Cn>,
    pub email: Option<Email>,
    pub sent_by: Option<SentBy>,
}

impl Organizer {
    #[must_use]
    pub fn with_cn(mut self, cn: impl Into<Cn>) -> Self {
        self.cn = Some(cn.into());
        self
    }

    #[must_use]
    pub fn with_email(mut self, email: impl Into<Email>) -> Self {
        self.email = Some(email.into());
        self
    }

    #[must_use]
    pub fn with_sent_by(mut self, sent_by: impl Into<SentBy>) -> Self {
        self.sent_by = Some(sent_by.into());
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
            email: None,
            sent_by: None,
        }
    }
}

impl IcsRead<Property> for Organizer {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut cn = None;
        let mut email = None;
        let mut sent_by = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "CN", &mut cn)
                || e.try_param(r, "EMAIL", &mut email)
                || e.try_param(r, "SENT-BY", &mut sent_by)
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
            email,
            cn,
            sent_by,
        })
    }
}

impl IcsWrite<Property> for Organizer {
    fn write(&self, w: &mut IcsWriter) {
        w.param_opt("CN", self.cn.as_ref());
        w.param_opt("EMAIL", self.email.as_ref());
        w.param_opt("SENT-BY", self.sent_by.as_ref());
        w.raw(":");
        w.value(&self.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":mailto:someone@somewhere.com")]
    #[test_case(":https://somewhere.com")]
    #[test_case(";CN=Someone At Somewhere:mailto:someone@somewhere.com")]
    #[test_case(";CN=Someone At Somewhere:https://somewhere.com")]
    #[test_case(";EMAIL=someone@somewhere.com:mailto:localhost")]
    #[test_case(";SENT-BY=\"mailto:someone-else@somewhere.com\":localhost")]
    fn smoke(s: &str) {
        assert_trip!(s, Organizer as Property);
    }

    #[test]
    fn invalid_sent_by() {
        let (obj, msgs) = Organizer::from_str_ex(
            ";SENT-BY=\"Spanish\tInquisition\":mailto:bar@localhost",
            Property,
        );

        assert_eq!(
            Some(Organizer::from(CalAddress::Email(EmailAddress::from(
                "bar@localhost"
            )))),
            obj
        );

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::one((1, 11))),
                body: "expected an email address (mailto:)".into(),
                kind: ReadMsgKind::Error,
                context: vec![
                    Spanned {
                        span: Span::one((1, 10)),
                        value: "`SentBy`".into(),
                    },
                    Spanned {
                        span: Span::one((1, 11)),
                        value: "`EmailAddress`".into(),
                    },
                ],
            }],
            msgs
        );
    }
}
