use super::*;

/// Version.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.7.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    Two,
}

impl IcsRead<Property> for Version {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        let value = r.spanned(|r| Some(r.rest()))?;
        let (span, value) = (value.span, value.as_str());

        if value == "2" || value.starts_with("2.") {
            Some(Version::Two)
        } else {
            r.error(span, format!("unknown version `{value}`"));
            None
        }
    }

    fn reasonable_default() -> Option<Self> {
        Some(Version::Two)
    }
}

impl IcsWrite<Property> for Version {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            Version::Two => {
                w.raw(":2.0");
            }
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    impl<'a> FromPhpZval<'a> for Version {
        const TYPE: PhpDataType = PhpDataType::String;

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            if zval.str()? == "Two" {
                Some(Version::Two)
            } else {
                None
            }
        }
    }

    impl IntoPhpZval for Version {
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
        assert_trip!(":2.0", Version as Property);
    }
}
