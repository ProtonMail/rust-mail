use std::collections::HashSet;

use anyhow::Context;
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
#[derive(Debug, Default)]
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

impl TryFrom<&IcalProperty> for Name {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::N));
        };

        let mut values = split_list(value, ';').into_iter();
        let mut next = || {
            values
                .next()
                .context("Too little args in Name")
                .map(|x| ListComponent::try_from(&*x).unwrap_or_default())
        };

        let mut result = Self {
            last: next()?,
            first: next()?,
            additional: next()?,
            prefix: next()?,
            suffix: next()?,
            ..Default::default()
        };

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
