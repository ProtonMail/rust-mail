use super::*;

/// Time transparency.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.2.7>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, EnumString)]
pub enum Transp {
    #[default]
    Opaque,
    Transparent,
}

impl IcsRead<Property> for Transp {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("OPAQUE") {
            Some(Transp::Opaque)
        } else if value.eq_ignore_ascii_case("TRANSPARENT") {
            Some(Transp::Transparent)
        } else {
            r.error(span, format!("unknown time transparency `{value}`"));
            None
        }
    }
}

impl IcsWrite<Property> for Transp {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(match self {
            Transp::Opaque => ":OPAQUE",
            Transp::Transparent => ":TRANSPARENT",
        });
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Transp {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            // Utilizing EnumString's impl
            <Self as std::str::FromStr>::from_str(zval.str()?).ok()
        }
    }

    impl IntoPhpZval for Transp {
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

    #[test_case(Transp::Opaque, ":OPAQUE")]
    #[test_case(Transp::Transparent, ":TRANSPARENT")]
    fn smoke(obj: Transp, str: &str) {
        assert_eq!(str, obj.to_string(Property));
        assert_trip!(str, Transp as Property);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new((1, 2), (1, 7))),
            body: "unknown time transparency `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Transp::from_str(":foobar", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
