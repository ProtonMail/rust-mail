use super::*;

/// Organizer.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.4.3>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Organizer {
    pub address: CalAddress,
    pub cn: Option<Cn>,
}

impl Organizer {
    #[must_use]
    pub fn with_cn(mut self, cn: impl Into<Cn>) -> Self {
        self.cn = Some(cn.into());
        self
    }
}

impl<T> From<T> for Organizer
where
    T: Into<CalAddress>,
{
    fn from(address: T) -> Self {
        Self {
            address: address.into(),
            cn: None,
        }
    }
}

impl IcsRead<Property> for Organizer {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut cn = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "CN", &mut cn) {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r);
        }

        Some(Self {
            address: r.value()?,
            cn,
        })
    }
}

impl IcsWrite<Property> for Organizer {
    fn write(&self, w: &mut IcsWriter) {
        w.param_opt("CN", self.cn.as_ref());
        w.raw(":");
        w.value(&self.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(":someone@somewhere.com")]
    #[test_case(":https://somewhere.com")]
    #[test_case(";CN=Someone At Somewhere:someone@somewhere.com")]
    fn smoke(s: &str) {
        assert_trip!(s, Organizer as Property);
    }
}
