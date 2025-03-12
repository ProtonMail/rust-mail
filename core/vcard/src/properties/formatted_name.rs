use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::language::Language;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::{
    any_debug, loop_debug, optional_debug, validate_parameters, VcardProperty,
};
use crate::validation::get_property_kind;
use crate::values::text::{is_text_value, Text};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the formatted text corresponding to the name of the object the vCard represents.
#[derive(Clone)]
pub struct FormattedName {
    pub value: Text,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Language
    pub language: Option<Language>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Media type linked by the value
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl FormattedName {
    /// Create a new FN property without any parameter or group (no validation are done on the value)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: Text::new_unchecked(value),
            value_type: None,
            r#type: HashSet::new(),
            language: None,
            alternative_id: None,
            pid: None,
            preference: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new FN property without any parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid text value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Text::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Fn))?,
            value_type: None,
            r#type: HashSet::new(),
            language: None,
            alternative_id: None,
            pid: None,
            preference: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for FormattedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FormattedName {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, LANG, language);
        optional_debug!(self, f, ALTID, alternative_id);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for FormattedName {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Fn));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?;
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Fn))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Fn,
                            parameter_type,
                        ))
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for FormattedName {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `FN` property
///
/// # Errors
///   * if property value is not a valid text
///   * if any of the parameters is not valid
pub fn validate_fn(property: &IcalProperty) -> VcardValidationResult<()> {
    // FN-param = "VALUE=text" / type-param / language-param / altid-param / pid-param / pref-param / any-param
    // FN-value = text
    if let Some(value) = &property.value {
        if is_text_value(value) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Type,
                    ParameterType::Language,
                    ParameterType::AltId,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::Any
                ),
            )?;
        }
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
