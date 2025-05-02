use super::*;

/// Time transparency.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.2.7>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transp {
    #[default]
    Opaque,
    Transparent,
}

impl Read<Property> for Transp {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

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

impl Write<Property> for Transp {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            Transp::Opaque => ":OPAQUE",
            Transp::Transparent => ":TRANSPARENT",
        });
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
            at: Some(Span::new(1, 7)),
            msg: "unknown time transparency `foobar`".into(),
            kind: ReadMsgKind::Error,
            context: Vec::new(),
        }];

        let actual = Transp::from_str(":foobar", Property).unwrap_err();

        assert_eq!(expected, actual);
    }
}
