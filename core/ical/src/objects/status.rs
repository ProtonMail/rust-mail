use super::*;

/// Status.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.11>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Tentative,
    Confirmed,
    Cancelled,
}

impl Read<Property> for Status {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("TENTATIVE") {
            Some(Status::Tentative)
        } else if value.eq_ignore_ascii_case("CONFIRMED") {
            Some(Status::Confirmed)
        } else if value.eq_ignore_ascii_case("CANCELLED") {
            Some(Status::Cancelled)
        } else {
            r.error(span, format!("unknown status `{value}`"));
            None
        }
    }
}

impl Write<Property> for Status {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            Status::Tentative => ":TENTATIVE",
            Status::Confirmed => ":CONFIRMED",
            Status::Cancelled => ":CANCELLED",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(Status::Tentative, ":TENTATIVE")]
    #[test_case(Status::Confirmed, ":CONFIRMED")]
    #[test_case(Status::Cancelled, ":CANCELLED")]
    fn smoke(obj: Status, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Status as Property);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new(1, 7)),
            msg: "unknown status `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Status::from_str(":foobar", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
