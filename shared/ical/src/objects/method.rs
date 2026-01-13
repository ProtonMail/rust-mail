use super::*;

/// Method.
///
/// <https://www.rfc-editor.org/rfc/rfc5546.html#section-3.2>
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
pub enum Method {
    Publish,
    Request,
    Reply,
    Add,
    Cancel,
    Refresh,
    Counter,
    DeclineCounter,
}

impl IcsRead<Property> for Method {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("PUBLISH") {
            Some(Method::Publish)
        } else if value.eq_ignore_ascii_case("REQUEST") {
            Some(Method::Request)
        } else if value.eq_ignore_ascii_case("REPLY") {
            Some(Method::Reply)
        } else if value.eq_ignore_ascii_case("ADD") {
            Some(Method::Add)
        } else if value.eq_ignore_ascii_case("CANCEL") {
            Some(Method::Cancel)
        } else if value.eq_ignore_ascii_case("REFRESH") {
            Some(Method::Refresh)
        } else if value.eq_ignore_ascii_case("COUNTER") {
            Some(Method::Counter)
        } else if value.eq_ignore_ascii_case("DECLINECOUNTER") {
            Some(Method::DeclineCounter)
        } else {
            r.error(span, format!("unknown method `{value}`"));
            None
        }
    }
}

impl IcsWrite<Property> for Method {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Method::Publish => ":PUBLISH",
            Method::Request => ":REQUEST",
            Method::Reply => ":REPLY",
            Method::Add => ":ADD",
            Method::Cancel => ":CANCEL",
            Method::Refresh => ":REFRESH",
            Method::Counter => ":COUNTER",
            Method::DeclineCounter => ":DECLINECOUNTER",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Method {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for Method {
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

    #[test_case(Method::Publish, ":PUBLISH")]
    #[test_case(Method::Request, ":REQUEST")]
    #[test_case(Method::Reply, ":REPLY")]
    #[test_case(Method::Add, ":ADD")]
    #[test_case(Method::Cancel, ":CANCEL")]
    #[test_case(Method::Refresh, ":REFRESH")]
    #[test_case(Method::Counter, ":COUNTER")]
    #[test_case(Method::DeclineCounter, ":DECLINECOUNTER")]
    fn test_name(obj: Method, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Method as Property);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new((1, 2), (1, 7))),
            body: "unknown method `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Method::from_str(":foobar", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
