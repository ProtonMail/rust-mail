use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::Parameters;
use crate::parameters::preference::Preference;
use crate::properties::VcardProperty;
use crate::values::x_name::XName;
use crate::{PropertyKind, VCardError, VCardResult};

/// The properties and parameters defined by this document can be extended.  Non-standard, private
/// properties and parameters with a name starting with "X-" may be defined bilaterally between two
/// cooperating agents without outside registration or standardization.

#[derive(Clone, Debug)]
pub struct Xtended {
    /// Name of the property
    pub name: XName,
    /// Value: comma and semicolon are used to parse vCard there presence here must be taken with care
    pub value: Option<String>,
    /// Parameters
    pub parameters: Parameters,
    /// Group
    pub group: Option<String>,
}

impl Xtended {
    /// Create a new Extended property (the X- is automatically added) no check are done
    #[must_use]
    pub fn new_unchecked(name: &str, value: Option<String>) -> Self {
        Self {
            name: XName::new_unchecked(&format!("X-{name}")),
            value,
            parameters: Parameters::new(),
            group: None,
        }
    }

    /// Try to create a new Extended property (the X- is automatically added)
    pub fn new_validated(name: &str, value: Option<String>) -> VCardResult<Self> {
        Ok(Self {
            name: XName::new_validated(&format!("X-{name}")).map_err(
                VCardError::from_value_error(PropertyKind::Extended(name.to_owned())),
            )?,
            value,
            parameters: Parameters::new(),
            group: None,
        })
    }
}

impl TryFrom<&IcalProperty> for Xtended {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let (group, name) = if let Some((group, name)) = property.name.split_once('.') {
            (Some(group.to_owned()), name)
        } else {
            (None, property.name.as_str())
        };
        Ok(Self {
            name: XName::try_from(name).map_err(VCardError::from_value_error(
                PropertyKind::Extended(name.to_owned()),
            ))?,
            value: property.value.clone(),
            parameters: Parameters::try_from(property).map_err(
                VCardError::from_parameter_error(PropertyKind::Extended(name.to_owned())),
            )?,
            group,
        })
    }
}

impl VcardProperty for Xtended {
    fn get_preference(&self) -> Option<Preference> {
        self.parameters.preference
    }
}
