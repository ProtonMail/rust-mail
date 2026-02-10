use super::*;

/// Status.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.1.11>
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
pub enum Status {
    Tentative,
    Confirmed,
    Cancelled,
}

impl IcsRead<Property> for Status {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("TENTATIVE") {
            Some(Status::Tentative)
        } else if value.eq_ignore_ascii_case("CONFIRMED") {
            Some(Status::Confirmed)
        } else if value.eq_ignore_ascii_case("CANCELLED") {
            Some(Status::Cancelled)
        } else if value.eq_ignore_ascii_case("ACCEPTED") || value.eq_ignore_ascii_case("UPDATED") {
            // Happens on production and we don't want the parser to fail in
            // this case; this will be later properly fixed with the sugery
            // process
            r.warn(span, format!("quirky status `{value}`"));
            None
        } else {
            r.error(span, format!("unknown status `{value}`"));
            None
        }
    }
}

impl IcsWrite<Property> for Status {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Status::Tentative => ":TENTATIVE",
            Status::Confirmed => ":CONFIRMED",
            Status::Cancelled => ":CANCELLED",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Status {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for Status {
        const TYPE: PhpDataType = PhpDataType::String;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            zval.set_string(&format!("{self:?}"), persistent)
        }
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
        let actual = Status::from_str(":foobar", Property).unwrap_err();

        let expected = vec![ReadMsg {
            at: Some(Span::new((1, 2), (1, 7))),
            body: "unknown status `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        assert_eq!(expected, actual);
    }

    #[test]
    fn accepted() {
        let (obj, msgs) = Status::from_str_ex(":ACCEPTED", Property);

        assert_eq!(None, obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 2), (1, 9))),
                body: "quirky status `ACCEPTED`".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs
        );
    }

    #[test]
    fn updated() {
        let (obj, msgs) = Status::from_str_ex(":UPDATED", Property);

        assert_eq!(None, obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 2), (1, 8))),
                body: "quirky status `UPDATED`".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            }],
            msgs
        );
    }
}
