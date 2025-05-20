use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;
use tracing::warn;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::calendar_scale::CalendarScale;
use crate::parameters::language::Language;
use crate::parameters::value::ValueType;
use crate::properties::{get_value_type, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::date_and_or_time::{MaybeDateAndOrTime, is_date_and_or_time_value};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the birthdate of the object the vCard represents.
#[derive(Clone, Default, Debug)]
pub struct Birthday {
    /// Value (ex: --0415 or circa 1800)
    pub value: MaybeDateAndOrTime,
    /// type of the value (here nothing or "date-and-or-time" of "text")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Calendar scale
    pub calendar_scale: Option<CalendarScale>,
    /// Language
    pub language: Option<Language>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Birthday {
    /// Try to create a new BDAY property
    ///
    /// # Errors
    ///   * if given value is neither a date-and-or-time value nor a text
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: value.into(),
            ..Default::default()
        })
    }
}

impl TryFrom<&IcalProperty> for Birthday {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::BDay));
        };
        let mut value_type = None;
        let mut alternative_id = None;
        let mut calendar_scale = None;
        let mut language = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::CalScale => {
                        calendar_scale = Some(
                            CalendarScale::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::Language => {
                        language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::BDay))?,
                        );
                    }
                    parameter_type => {
                        warn!("Unexpected parameter: {parameter_type:?}");
                    }
                }
            }
        }

        Ok(Self {
            value: value.into(),
            value_type,
            alternative_id,
            calendar_scale,
            language,
            any,
            group: group_from_name(&property.name),
        })
    }
}

/// Validate that the given `property` respect the format for a `BDAY` property
///
/// # Errors
///   * if property value is not a date-and-or-time value or a text
///   * if any parameter is not valid
pub fn validate_bday(property: &IcalProperty) -> VcardValidationResult<()> {
    // BDAY-param = BDAY-param-date / BDAY-param-text
    // BDAY-value = date-and-or-time / text
    //   ; Value and parameter MUST match.
    //
    // BDAY-param-date = "VALUE=date-and-or-time"
    // BDAY-param-text = "VALUE=text" / language-param
    //
    // BDAY-param =/ altid-param / calscale-param / any-param
    //   ; calscale-param can only be present when BDAY-value is
    //   ; date-and-or-time and actually contains a date or date-time.

    if let Some(value) = &property.value {
        let value_type = if let Some(value_type) = get_value_type(property)? {
            let validated = match value_type {
                ValueType::DateAndOrTime => is_date_and_or_time_value(value),
                ValueType::Text => true,
                _ => false,
            };
            if !validated {
                return Err(VcardValidationError::InvalidPropertyValue(
                    get_property_kind(&property.name)?,
                ));
            }
            value_type
        } else if is_date_and_or_time_value(value) {
            ValueType::DateAndOrTime
        } else {
            ValueType::Text
        };

        validate_parameters(
            property,
            value_type,
            &hash_set!(
                ParameterType::Value,
                ParameterType::AltId,
                if matches!(value_type, ValueType::DateAndOrTime) {
                    ParameterType::CalScale
                } else {
                    ParameterType::Language
                },
                ParameterType::Any
            ),
        )?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
