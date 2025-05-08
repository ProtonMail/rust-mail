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

/// To specify an image or photograph information that annotates some aspect of the object the vCard
/// represents.
#[derive(Clone)]
pub struct Photo {
    /// uri (ex: <http://www.example.com/pub/photos/jqpublic.gif> or <data:image/jpeg;base64,MIICajCCAdOgAwIBAg...>)
    pub value: Uri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl Photo {
    /// Create a new PHOTO property without any parameter or group
    #[must_use]
    pub fn new(url: Url) -> Self {
        Self {
            value: Uri::new(url),
            value_type: None,
            alternative_id: None,
            r#type: HashSet::new(),
            media_type: None,
            preference: None,
            pid: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new PHOTO property without any parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid uri value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Photo))?,
            value_type: None,
            alternative_id: None,
            r#type: HashSet::new(),
            media_type: None,
            preference: None,
            pid: None,
            any: HashSet::new(),
            group: None,
        })
    }
}

impl Debug for Photo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Photo {{{:?}", self.value)?;
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

impl TryFrom<&IcalProperty> for Photo {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Photo));
        };

        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?;
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Photo))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Photo,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Photo {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `PHOTO` property
///
/// # Errors
///   * if property value is not a valid uri
///   * if any of the parameters is not valid
pub fn validate_photo(property: &IcalProperty) -> VcardValidationResult<()> {
    // PHOTO-param = "VALUE=uri" / altid-param / type-param / mediatype-param / pref-param / pid-param / any-param
    // PHOTO-value = URI
    if let Some(value) = &property.value {
        if Url::parse(value).is_ok() {
            validate_parameters(
                property,
                ValueType::Uri,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::AltId,
                    ParameterType::Type,
                    ParameterType::MediaType,
                    ParameterType::Pref,
                    ParameterType::Pid,
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
