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
use crate::values::text_list::{is_text_list_value, TextList};
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the text corresponding to the nickname of the object the vCard represents.
#[derive(Clone)]
pub struct Nickname {
    /// Value (ex: Jim,Jimmie)
    pub value: TextList,
    /// type of the value (here nothing or "text")
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

impl Nickname {
    /// Create a new NICKNAME property without any parameter or group (no check is done)
    #[must_use]
    pub fn new_unchecked(value: &[String]) -> Self {
        Self {
            value: TextList::new_unchecked(value),
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

    /// Try to create a new NICKNAME property without any parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid text value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: TextList::try_from(value)
                .map_err(VCardError::from_value_error(PropertyKind::Nickname))?,
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

impl Debug for Nickname {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Nickname {{{:?}", self.value)?;
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

impl TryFrom<&IcalProperty> for Nickname {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Nickname));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type =
                            Some(ValueType::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?);
                    }
                    ParameterType::Pid => {
                        result.pid =
                            Some(Pid::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?);
                    }
                    ParameterType::Pref => {
                        result.preference =
                            Some(Preference::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?);
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Nickname))?;
                    }
                    ParameterType::Language => {
                        result.language =
                            Some(Language::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?);
                    }
                    ParameterType::AltId => {
                        result.alternative_id =
                            Some(AlternativeId::try_from(values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?);
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice()).map_err(
                                VCardError::from_parameter_error(PropertyKind::Nickname),
                            )?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Nickname,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Nickname {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `NICKNAME` property
///
/// # Errors
///   * if property value is not a text value
///   * if any of the parameters is not valid
pub fn validate_nickname(property: &IcalProperty) -> VcardValidationResult<()> {
    // NICKNAME-param = "VALUE=text" / type-param / language-param / altid-param / pid-param / pref-param / any-param
    // NICKNAME-value = text-list
    if let Some(value) = &property.value {
        if is_text_list_value(value) {
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
