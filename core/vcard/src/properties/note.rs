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

/// To specify supplemental information or a comment that is associated with the vCard.
#[derive(Clone)]
pub struct Note {
    /// Value
    pub value: Text,
    /// type of the value (here nothing or "text")
    pub value_type: Option<ValueType>,
    /// Language
    pub language: Option<Language>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Note {
    /// Create a new NOTE property without any parameter or group (No check is done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: Text::new_unchecked(value),
            value_type: None,
            language: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new NOTE property without any parameter or group
    ///
    /// # Errors
    ///   * if givn value is not a valid text value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Text::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Note))?,
            value_type: None,
            language: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for Note {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Note {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, LANG, language);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for Note {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Note));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Note))?;
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Note))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Note,
                            parameter_type,
                        ))
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Note {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `NOTE` property
///
/// # Errors
///   * if property value is not a valid text value
///   * if any parameter is not valid
pub fn validate_note(property: &IcalProperty) -> VcardValidationResult<()> {
    // NOTE-param = "VALUE=text" / language-param / pid-param / pref-param / type-param / altid-param / any-param
    // NOTE-value = text
    if let Some(value) = &property.value {
        if is_text_value(value) {
            validate_parameters(
                property,
                ValueType::Text,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Language,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::Type,
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
