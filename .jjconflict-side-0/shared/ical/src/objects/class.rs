use super::*;

/// Classification.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.1.3>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, EnumString)]
pub enum Class {
    #[default]
    Public,
    Private,
    Confidential,
}

impl IcsRead<Property> for Class {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("PUBLIC") {
            Some(Class::Public)
        } else if value.eq_ignore_ascii_case("PRIVATE") {
            Some(Class::Private)
        } else if value.eq_ignore_ascii_case("CONFIDENTIAL") {
            Some(Class::Confidential)
        } else {
            r.error(span, format!("unknown classification `{value}`"));

            // > Applications MUST treat x-name and iana-token values they
            // > don't recognize the same way as they would the PRIVATE value.
            Some(Class::Private)
        }
    }
}

impl IcsWrite<Property> for Class {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Class::Public => ":PUBLIC",
            Class::Private => ":PRIVATE",
            Class::Confidential => ":CONFIDENTIAL",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Class {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for Class {
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

    #[test_case(Class::Public, ":PUBLIC")]
    #[test_case(Class::Private, ":PRIVATE")]
    #[test_case(Class::Confidential, ":CONFIDENTIAL")]
    fn smoke(obj: Class, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Class as Property);
    }

    #[test]
    fn unknown() {
        let (obj, msgs) = Class::from_str_ex(":foobar", Property);

        assert_eq!(Some(Class::Private), obj);

        assert_eq!(
            vec![ReadMsg {
                at: Some(Span::new((1, 2), (1, 7))),
                body: "unknown classification `foobar`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            }],
            msgs,
        );
    }
}
