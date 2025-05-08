use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::{
    VcardProperty, any_debug, loop_debug, optional_debug, validate_parameters,
};
use crate::validation::get_property_kind;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the language(s) that may be used for contacting the entity associated with the vCard.
#[derive(Clone)]
pub struct Language {
    /// value
    pub value: String,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
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

impl Language {
    /// Create a new LANG property without any parameter or group
    #[must_use]
    pub fn new(value: String) -> Self {
        Self {
            value,
            value_type: None,
            pid: None,
            preference: None,
            alternative_id: None,
            r#type: HashSet::new(),
            any: HashSet::new(),
            group: None,
        }
    }
}

impl Debug for Language {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Language {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}",)
    }
}

impl TryFrom<&IcalProperty> for Language {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Lang));
        };
        let mut result = Self {
            value: value.clone(),
            value_type: None,
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
                                .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?;
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Lang))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Lang,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Language {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `LANG` property
///
/// # Errors
///   * if property value is not a valid language-tag value
///   * if any of the parameters is not valid
pub fn validate_lang(property: &IcalProperty) -> VcardValidationResult<()> {
    // LANG-param = "VALUE=language-tag" / pid-param / pref-param / altid-param / type-param / any-param
    // LANG-value = Language-Tag
    if property.value.is_some() {
        validate_parameters(
            property,
            ValueType::LanguageTag,
            &hash_set!(
                ParameterType::Value,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::AltId,
                ParameterType::Type,
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
