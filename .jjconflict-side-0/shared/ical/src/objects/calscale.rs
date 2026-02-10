use super::*;

/// Calendar scale.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.7.1>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalScale {
    #[default]
    Gregorian,
}

impl IcsRead<Property> for CalScale {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("GREGORIAN") {
            Some(CalScale::Gregorian)
        } else {
            r.error(span, format!("unknown calscale `{value}`"));
            None
        }
    }

    fn reasonable_default() -> Option<Self> {
        Some(CalScale::Gregorian)
    }
}

impl IcsWrite<Property> for CalScale {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            CalScale::Gregorian => {
                w.raw(":GREGORIAN");
            }
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for CalScale {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            if zval.str()? == "Gregorian" {
                Some(CalScale::Gregorian)
            } else {
                None
            }
        }
    }

    impl IntoPhpZval for CalScale {
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

    #[test]
    fn smoke() {
        assert_trip!(":GREGORIAN", CalScale as Property);
    }
}
