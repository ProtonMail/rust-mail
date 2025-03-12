use std::fmt::{Debug, Formatter};

use crate::errors::{VCardValueError, VCardValueResult};
use crate::values::check_list;
use crate::values::component::{Component, is_component_value};
use crate::vcard::split_list;

/// A list of component values
#[derive(Clone, PartialEq)]
pub struct ListComponent(pub(crate) Vec<Component>);

impl ListComponent {
    /// Create a new `ListComponent`
    #[must_use]
    pub fn new(values: &[Component]) -> Self {
        Self(values.to_vec())
    }

    /// Try to create a new `ListComponent` from given str
    ///
    /// # Errors
    ///   * If not valid
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }

    /// Check if this `ListComponent` is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Debug for ListComponent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LC({:?})", self.0)
    }
}

impl TryFrom<&str> for ListComponent {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        let values: Result<_, _> = split_list(value, ',')
            .iter()
            .map(|v| TryInto::<Component>::try_into(v.as_str()))
            .collect();
        Ok(Self(values?))
    }
}

/// Validate that given `value` respect format for `list-component` values
pub fn is_list_component_value(value: &str) -> bool {
    // list-component = component *("," component)

    value.is_empty() || check_list(value, is_component_value, ',').is_some()
}
