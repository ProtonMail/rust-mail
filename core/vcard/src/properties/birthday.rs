use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::calendar_scale::CalendarScale;
use crate::parameters::language::Language;
use crate::parameters::value::ValueType;
use crate::properties::{any_debug, get_value_type, optional_debug, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::date_and_or_time::{DateAndOrTimeValue, is_date_and_or_time_value};
use crate::values::text::{Text, is_text_value};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the birthdate of the object the vCard represents.
#[derive(Clone)]
pub struct Birthday {
    /// Value (ex: --0415 or circa 1800)
    pub value: BirthdayValue,
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
            value: BirthdayValue::try_from(value)?,
            value_type: None,
            alternative_id: None,
            calendar_scale: None,
            language: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for Birthday {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Birthday {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, CALSCALE, calendar_scale);
        optional_debug!(self, f, ALTID, alternative_id);
        optional_debug!(self, f, LANG, language);
        optional_debug!(self, f, VALUE, value_type);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
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
                            Language::try_from(values.as_slice())
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
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::BDay,
                            parameter_type,
                        ));
                    }
                }
            }
        }

        let real_value_type = if let Some(value_type) = value_type {
            value_type
        } else if is_date_and_or_time_value(value) {
            ValueType::DateAndOrTime
        } else if is_text_value(value) {
            ValueType::Text
        } else {
            return Err(VCardError::InvalidValue(PropertyKind::BDay, value.clone()));
        };
        let value = match real_value_type {
            ValueType::DateAndOrTime => BirthdayValue::DateAndOrTime(
                DateAndOrTimeValue::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::BDay))?,
            ),
            ValueType::Text => BirthdayValue::Text(
                Text::try_from(value.as_str())
                    .map_err(VCardError::from_value_error(PropertyKind::BDay))?,
            ),
            _ => {
                return Err(VCardError::InvalidValue(
                    PropertyKind::BDay,
                    value.to_owned(),
                ));
            }
        };
        Ok(Self {
            value,
            value_type,
            alternative_id,
            calendar_scale,
            language,
            any,
            group: group_from_name(&property.name),
        })
    }
}

/// Possible value type for the BDAY property
#[derive(Clone, PartialEq)]
pub enum BirthdayValue {
    /// A Date or Time or Datetime
    DateAndOrTime(DateAndOrTimeValue),
    /// Free text
    Text(Text),
}

impl BirthdayValue {
    /// Try to create a new Value for BDAY property
    ///
    /// # Errors
    ///   * if given value is neither a date-and-or-time value nor a text
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for BirthdayValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(v) => write!(f, "{v:?}"),
            Self::DateAndOrTime(v) => write!(f, "{v:?}"),
        }
    }
}

impl TryFrom<&str> for BirthdayValue {
    type Error = VCardError;

    fn try_from(value: &str) -> VCardResult<Self> {
        if is_date_and_or_time_value(value) {
            Ok(Self::DateAndOrTime(
                DateAndOrTimeValue::try_from(value)
                    .map_err(VCardError::from_value_error(PropertyKind::BDay))?,
            ))
        } else if is_text_value(value) {
            Ok(Self::Text(Text::try_from(value).map_err(
                VCardError::from_value_error(PropertyKind::BDay),
            )?))
        } else {
            Err(VCardError::InvalidValue(
                PropertyKind::BDay,
                value.to_owned(),
            ))
        }
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
                ValueType::Text => is_text_value(value),
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
        } else if is_text_value(value) {
            ValueType::Text
        } else {
            return Err(VcardValidationError::InvalidPropertyValue(
                get_property_kind(&property.name)?,
            ));
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
