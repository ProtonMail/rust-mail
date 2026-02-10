use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::language::Language;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::sort_as::SortAs;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::component::Component;
use crate::vcard::{group_from_name, split_list};
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the organizational name and units associated with the vCard.
#[derive(Clone, Debug, Default)]
pub struct Organization {
    /// Hierarchic list of components
    pub values: Vec<Component>,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// Sort as
    pub sort_as: Option<SortAs>,
    /// Language
    pub language: Option<Language>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Organization {
    /// Create a new ORG property without any parameter or group
    #[must_use]
    pub fn new(values: Vec<Component>) -> Self {
        Self {
            values,
            value_type: None,
            sort_as: None,
            language: None,
            pid: None,
            preference: None,
            alternative_id: None,
            r#type: HashSet::new(),
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new ORG property
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self::new(Self::value_into_components(value)?))
    }

    fn value_into_components(value: &str) -> VCardResult<Vec<Component>> {
        split_list(value, ';')
            .iter()
            .map(|v| Component::try_from(v.as_str()))
            .collect::<Result<_, _>>()
            .map_err(VCardError::from_value_error(PropertyKind::Org))
    }
}

impl TryFrom<&IcalProperty> for Organization {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Org));
        };
        let values = Self::value_into_components(value)?;
        let mut result = Self {
            values,
            value_type: None,
            sort_as: None,
            language: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            alternative_id: None,
            any: HashSet::new(),
            group: group_from_name(&property.name),
        };
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Org))?;
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::SortAs => {
                        result.sort_as = Some(
                            SortAs::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Org))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Org,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Organization {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
