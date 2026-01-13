use super::*;

/// RSVP expectation.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.17>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rsvp(bool);

impl Rsvp {
    #[must_use]
    pub fn yes() -> Self {
        Self::from(true)
    }

    #[must_use]
    pub fn no() -> Self {
        Self::from(false)
    }

    #[must_use]
    pub fn as_bool(&self) -> bool {
        self.0
    }
}

impl From<bool> for Rsvp {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl IcsRead<Value> for Rsvp {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("TRUE") {
            Some(Self(true))
        } else if value.eq_ignore_ascii_case("FALSE") {
            Some(Self(false))
        } else {
            r.error(span, format!("unknown rsvp `{value}`"));
            None
        }
    }
}

impl IcsWrite<Value> for Rsvp {
    fn write(&self, w: &mut IcsWriter) {
        w.value(self.0);
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Rsvp {
        const TYPE: PhpDataType = PhpDataType::Bool;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            Some(Self(zval.bool()?))
        }
    }

    impl IntoPhpZval for Rsvp {
        const TYPE: PhpDataType = PhpDataType::Bool;
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, _: bool) -> PhpResult<()> {
            zval.set_bool(self.0);

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(Rsvp::yes(), "TRUE")]
    #[test_case(Rsvp::no(), "FALSE")]
    #[test_case(Rsvp::default(), "FALSE")]
    fn smoke(obj: Rsvp, str: &str) {
        assert_eq!(str, obj.to_string(Value));
        assert_trip!(str, Rsvp as Value);
    }

    #[test]
    fn unknown() {
        let expected = vec![ReadMsg {
            at: Some(Span::new((1, 1), (1, 6))),
            body: "unknown rsvp `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Rsvp::from_str("foobar", Value).unwrap_err();

        assert_eq!(expected, actual);
    }
}
