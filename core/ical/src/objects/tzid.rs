use super::*;

/// Time zone identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.1>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct TzId {
    // TODO Text / ParamValue (must not contain dquote)
    pub value: String,
}

impl TzId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<T> From<T> for TzId
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl Read<Property> for TzId {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self {
            value: r.value::<Text>()?.into_string(),
        })
    }
}

impl Write<Property> for TzId {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.raw(self.value.as_str());
    }
}

impl Read<Value> for TzId {
    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self {
            value: r.value::<ParamValue>()?.into_string(),
        })
    }
}

impl Write<Value> for TzId {
    fn write(&self, w: &mut Writer) {
        w.raw(self.value.as_str());
    }
}
