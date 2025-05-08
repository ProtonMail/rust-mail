use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use url::Url;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_generic::GenericType;
use crate::parameters::value::ValueType;
use crate::properties::{
    VcardProperty, any_debug, loop_debug, optional_debug, validate_parameters,
};
use crate::validation::get_property_kind;
use crate::values::uri::Uri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify the URI for the busy time associated with the object that the vCard represents.
#[derive(Clone)]
pub struct FbUrl {
    /// Value (ex: <http://www.example.com/busy/janedoe>, <ftp://example.com/busy/project-a.ifb>)
    pub value: Uri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl FbUrl {
    /// Create a new FBURL without parameter or group
    #[must_use]
    pub fn new(url: Url) -> Self {
        Self {
            value: Uri::new(url),
            value_type: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new FBURL from given value without parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid uri
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::FbUrl))?,
            value_type: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for FbUrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FbUrl {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        loop_debug!(self, f, TYPE, r#type);
        optional_debug!(self, f, MEDIATYPE, media_type);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for FbUrl {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::FbUrl));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?;
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::FbUrl))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::FbUrl,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for FbUrl {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `FBURL` property
/// N.B.: FBURL is for Busy time
///
/// # Errors
///   * if property value is not a valid uri
pub fn validate_fburl(property: &IcalProperty) -> VcardValidationResult<()> {
    // FBURL-param = "VALUE=uri" / pid-param / pref-param / type-param / mediatype-param / altid-param / any-param
    // FBURL-value = URI
    if let Some(value) = &property.value {
        if Url::parse(value).is_ok() {
            validate_parameters(
                property,
                ValueType::Uri,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::Type,
                    ParameterType::MediaType,
                    ParameterType::AltId,
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
