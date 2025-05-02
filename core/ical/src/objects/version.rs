use super::*;

/// Version.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.7.4>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    Two,
}

impl Read<Property> for Version {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

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

impl Write<Property> for Version {
    fn write(&self, w: &mut Writer) {
        match self {
            Version::Two => {
                w.raw(":2.0");
            }
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
