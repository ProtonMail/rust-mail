use super::*;

/// Time zone identifier.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.3.1>
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct TzId {
    // Usually we'd use `Text` or `ParamValue`, but `TzId` is a bit awkward in
    // that it can appear in both positions - so, for convenience, let's just
    // use `String`.
    pub value: String,
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

impl IcsRead<Property> for TzId {
    fn read(r: &mut IcsReader) -> Option<Self> {
        r.burn_params()?;

        Some(Self {
            value: r.value::<Text>()?.into_string(),
        })
    }
}

impl IcsWrite<Property> for TzId {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(":");
        w.raw(self.value.as_str());
    }
}

impl IcsRead<Value> for TzId {
    fn read(r: &mut IcsReader) -> Option<Self> {
        Some(Self {
            value: r.value::<ParamValue>()?.into_string(),
        })
    }
}

impl IcsWrite<Value> for TzId {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(self.value.as_str());
    }
}
