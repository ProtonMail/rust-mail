use super::*;

/// Calendar user type.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.3>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, EnumString)]
pub enum CuType {
    #[default]
    Individual,
    Group,
    Resource,
    Room,
    Unknown,
}

impl IcsRead<Value> for CuType {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("INDIVIDUAL") {
            Some(CuType::Individual)
        } else if value.eq_ignore_ascii_case("GROUP") {
            Some(CuType::Group)
        } else if value.eq_ignore_ascii_case("RESOURCE") {
            Some(CuType::Resource)
        } else if value.eq_ignore_ascii_case("ROOM") {
            Some(CuType::Room)
        } else if value.eq_ignore_ascii_case("UNKNOWN") {
            Some(CuType::Unknown)
        } else {
            r.error(span, format!("unknown cutype `{value}`"));

            // > Applications MUST treat x-name and iana-token values they don't
            // > recognize the same way as they would the UNKNOWN value.
            Some(CuType::Unknown)
        }
    }
}

impl IcsWrite<Value> for CuType {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            CuType::Individual => "INDIVIDUAL",
            CuType::Group => "GROUP",
            CuType::Resource => "RESOURCE",
            CuType::Room => "ROOM",
            CuType::Unknown => "UNKNOWN",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for CuType {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for CuType {
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

    #[test_case(CuType::Individual, "INDIVIDUAL")]
    #[test_case(CuType::Group, "GROUP")]
    #[test_case(CuType::Resource, "RESOURCE")]
    #[test_case(CuType::Room, "ROOM")]
    #[test_case(CuType::Unknown, "UNKNOWN")]
    #[test_case(CuType::default(), "INDIVIDUAL")]
    fn smoke(obj: CuType, str: &str) {
        assert_eq!(obj, CuType::from_str(str, Value).unwrap());
        assert_trip!(str, CuType as Value);
    }

    #[test]
    fn unknown() {
        let (obj, msgs) = CuType::from_str_ex("foobar", Value);

        assert_eq!(Some(CuType::Unknown), obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 1), (1, 6))),
                body: "unknown cutype `foobar`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }
}
