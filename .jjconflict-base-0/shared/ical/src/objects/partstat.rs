use super::*;

/// Participation status.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.12>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, EnumString)]
pub enum PartStat {
    #[default]
    NeedsAction,
    Accepted,
    Declined,
    Tentative,
    Delegated,
}

impl IcsRead<Value> for PartStat {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("NEEDS-ACTION") {
            Some(PartStat::NeedsAction)
        } else if value.eq_ignore_ascii_case("ACCEPTED") {
            Some(PartStat::Accepted)
        } else if value.eq_ignore_ascii_case("DECLINED") {
            Some(PartStat::Declined)
        } else if value.eq_ignore_ascii_case("TENTATIVE") {
            Some(PartStat::Tentative)
        } else if value.eq_ignore_ascii_case("DELEGATED") {
            Some(PartStat::Delegated)
        } else {
            r.error(span, format!("unknown participation status `{value}`"));

            // > Applications MUST treat x-name and iana-token values they don't
            // > recognize the same way as they would the NEEDS-ACTION value.
            Some(PartStat::NeedsAction)
        }
    }
}

impl IcsWrite<Value> for PartStat {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            PartStat::NeedsAction => "NEEDS-ACTION",
            PartStat::Accepted => "ACCEPTED",
            PartStat::Declined => "DECLINED",
            PartStat::Tentative => "TENTATIVE",
            PartStat::Delegated => "DELEGATED",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for PartStat {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for PartStat {
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

    #[test_case(PartStat::NeedsAction, "NEEDS-ACTION")]
    #[test_case(PartStat::Accepted, "ACCEPTED")]
    #[test_case(PartStat::Declined, "DECLINED")]
    #[test_case(PartStat::Tentative, "TENTATIVE")]
    #[test_case(PartStat::Delegated, "DELEGATED")]
    fn smoke(obj: PartStat, str: &str) {
        assert_eq!(str, obj.to_string(Value));
        assert_trip!(str, PartStat as Value);
    }

    #[test]
    fn unknown() {
        let (obj, msgs) = PartStat::from_str_ex("foobar", Value);

        assert_eq!(Some(PartStat::NeedsAction), obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 1), (1, 6))),
                body: "unknown participation status `foobar`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }
}
