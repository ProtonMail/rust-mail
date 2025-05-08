use ical::generator::Property as IcalProperty;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use url::Url;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::ParameterType;
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::value::ValueType;
use crate::properties::{VcardProperty, any_debug, optional_debug, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::uri::Uri;
use crate::vcard::group_from_name;
use crate::{PropertyKind, VCardError, VCardResult};

/// To identify the source of directory information contained in the content type.
#[derive(Clone)]
pub struct Source {
    /// Value (ex: <ldap://ldap.example.com/cn=Babs%20Jensen,%20o=Babsco,%20c=US>)
    pub value: Uri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other Source property
    pub preference: Option<Preference>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this Source belong to
    pub group: Option<String>,
}

impl Source {
    /// Create a new SOURCE property without any parameter or group
    #[must_use]
    pub fn new(url: Url) -> Self {
        Self {
            value: Uri::new(url),
            value_type: None,
            pid: None,
            preference: None,
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new SOURCE property without any parameter or group
    ///
    /// # Errors
    ///   * the value is not a valid uri value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Source))?,
            value_type: None,
            pid: None,
            preference: None,
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for Source {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Source {{{:?}", self.value)?;
        optional_debug!(self, f, VALUE, value_type);
        optional_debug!(self, f, PID, pid);
        optional_debug!(self, f, PREF, preference);
        optional_debug!(self, f, MEDIATYPE, media_type);
        optional_debug!(self, f, ALTID, alternative_id);
        any_debug!(self, f, any);
        optional_debug!(self, f, group, group);
        write!(f, "}}")
    }
}

impl TryFrom<&IcalProperty> for Source {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Source));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Source))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Source,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Source {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `SOURCE` property
///
/// # Errors
///   * property value is not a valid uri
///   * any of the property is invalid
pub fn validate_source(property: &IcalProperty) -> VcardValidationResult<()> {
    // SOURCE-param = "VALUE=uri" / pid-param / pref-param / altid-param / mediatype-param / any-param
    // SOURCE-value = URI
    if let Some(value) = &property.value {
        if Url::parse(value).is_ok() {
            validate_parameters(
                property,
                ValueType::Uri,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::AltId,
                    ParameterType::MediaType,
                    ParameterType::Any
                ),
            )?;
        }
        return Ok(());
    }
    Err(VcardValidationError::InvalidPropertyValue(
        get_property_kind(&property.name)?,
    ))
}
