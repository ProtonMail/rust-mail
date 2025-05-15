use std::collections::HashSet;

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::language::Language;
use crate::parameters::sort_as::SortAs;
use crate::parameters::value::ValueType;
use crate::properties::validate_parameters;
use crate::validation::get_property_kind;
use crate::values::check_list;
use crate::values::list_component::{ListComponent, is_list_component_value};
use crate::vcard::{group_from_name, split_list};
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the components of the name of the object the vCard represents.
#[derive(Debug)]
pub struct Name {
    pub last: ListComponent,
    pub first: ListComponent,
    pub additional: ListComponent,
    /// honorific prefix like Dr, Mr, Don
    pub prefix: ListComponent,
    /// honorific suffix like `PhD`
    pub suffix: ListComponent,
    pub value_type: Option<ValueType>,
    pub sort_as: Option<SortAs>,
    pub language: Option<Language>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Name {
    /// Create a new N property without any parameter or group
    #[must_use]
    pub fn new(
        last: ListComponent,
        first: ListComponent,
        additional: ListComponent,
        prefix: ListComponent,
        suffix: ListComponent,
    ) -> Self {
        Self {
            last,
            first,
            additional,
            prefix,
            suffix,
            value_type: None,
            sort_as: None,
            language: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new N property without any parameter or group
    ///
    /// # Errors
    ///   * if any of the argument is not a valid list-component
    pub fn new_validated(
        last: &str,
        first: &str,
        additional: &str,
        prefix: &str,
        suffix: &str,
    ) -> VCardResult<Self> {
        Ok(Self::new(
            ListComponent::try_from(last).map_err(VCardError::from_value_error(PropertyKind::N))?,
            ListComponent::try_from(first)
                .map_err(VCardError::from_value_error(PropertyKind::N))?,
            ListComponent::try_from(additional)
                .map_err(VCardError::from_value_error(PropertyKind::N))?,
            ListComponent::try_from(prefix)
                .map_err(VCardError::from_value_error(PropertyKind::N))?,
            ListComponent::try_from(suffix)
                .map_err(VCardError::from_value_error(PropertyKind::N))?,
        ))
    }
}

impl TryFrom<&IcalProperty> for Name {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::N));
        };

        let values: Result<Vec<_>, _> = split_list(value, ';')
            .iter()
            .map(|v| ListComponent::try_from(v.as_str()))
            .collect();
        // N-value = list-component 4(";" list-component)
        // So ';;;;' is a valid value with 5 empty list-component
        let values: [ListComponent; 5] = values
            .map_err(VCardError::from_value_error(PropertyKind::N))?
            .try_into()
            .map_err(|_| VCardError::InvalidValue(PropertyKind::N, value.clone()))?;

        let mut result = Self::new(
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
            values[3].clone(),
            values[4].clone(),
        );
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::N))?,
                        );
                    }
                    ParameterType::SortAs => {
                        result.sort_as = Some(
                            SortAs::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::N))?,
                        );
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::N))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::N))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::N))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::N,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

/// Validate that the given `property` respect the format for a `N` property
///
/// # Errors
///   * if property value is not a valid list of 5 list-component values separated by semicolon
///   * if any of the parameters is not valid
pub fn validate_n(property: &IcalProperty) -> VcardValidationResult<()> {
    // N-param = "VALUE=text" / sort-as-param / language-param / altid-param / any-param
    // N-value = list-component 4(";" list-component)
    if let Some(value) = &property.value {
        if check_list(value, is_list_component_value, ';').is_some_and(|c| c == 5) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::SortAs,
                    ParameterType::Language,
                    ParameterType::AltId,
                    ParameterType::Any,
                ),
            )?;
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
        }
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
