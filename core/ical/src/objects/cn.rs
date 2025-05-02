use super::*;

/// Common name.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.2.2>
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cn {
    pub value: ParamValue,
}

impl<T> From<T> for Cn
where
    T: Into<ParamValue>,
{
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}
