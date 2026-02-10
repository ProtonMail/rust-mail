use super::*;

/// Participation role.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.16>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, EnumString)]
pub enum Role {
    Chair,
    #[default]
    ReqParticipant,
    OptParticipant,
    NonParticipant,
}

impl IcsRead<Value> for Role {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("CHAIR") {
            Some(Role::Chair)
        } else if value.eq_ignore_ascii_case("REQ-PARTICIPANT") {
            Some(Role::ReqParticipant)
        } else if value.eq_ignore_ascii_case("OPT-PARTICIPANT") {
            Some(Role::OptParticipant)
        } else if value.eq_ignore_ascii_case("NON-PARTICIPANT") {
            Some(Role::NonParticipant)
        } else {
            r.error(span, format!("unknown role `{value}`"));

            // > Applications MUST treat x-name and iana-token values they don't
            // > recognize the same way as they would the REQ-PARTICIPANT value.
            Some(Role::ReqParticipant)
        }
    }
}

impl IcsWrite<Value> for Role {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Role::Chair => "CHAIR",
            Role::ReqParticipant => "REQ-PARTICIPANT",
            Role::OptParticipant => "OPT-PARTICIPANT",
            Role::NonParticipant => "NON-PARTICIPANT",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Role {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for Role {
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

    #[test_case(Role::Chair, "CHAIR")]
    #[test_case(Role::ReqParticipant, "REQ-PARTICIPANT")]
    #[test_case(Role::OptParticipant, "OPT-PARTICIPANT")]
    #[test_case(Role::NonParticipant, "NON-PARTICIPANT")]
    fn smoke(obj: Role, str: &str) {
        assert_eq!(str, obj.to_string(Value));
        assert_trip!(str, Role as Value);
    }

    #[test]
    fn unknown() {
        let (obj, msgs) = Role::from_str_ex("foobar", Value);

        assert_eq!(Some(Role::ReqParticipant), obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 1), (1, 6))),
                body: "unknown role `foobar`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }
}
