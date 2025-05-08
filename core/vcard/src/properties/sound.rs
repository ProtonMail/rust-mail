use std::collections::HashSet;
use std::fmt::{Debug, Formatter};

use ical::generator::Property as IcalProperty;
use url::Url;
use velcro::hash_set;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::language::Language;
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

/// To specify a digital sound content information that annotates some aspect of the vCard.  This
/// property is often used to specify the proper pronunciation of the name property value of the
/// vCard.
#[derive(Clone)]
pub struct Sound {
    /// Value (ex: CID:JOHNQPUBLIC.part8.19960229T080000.xyzMail@example.com or data:audio/basic;base64,MIICajCCAdOgAwIBAgICBEUw...
    pub value: Uri,
    /// type of the value (here nothing or "uri")
    pub value_type: Option<ValueType>,
    /// Language
    pub language: Option<Language>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other SOUND property
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

impl Sound {
    /// Create a new SOUND property without any parameter or group
    #[must_use]
    pub fn new(url: Url) -> Self {
        Self {
            value: Uri::new(url),
            value_type: None,
            language: None,
            pid: None,
            preference: None,
            r#type: HashSet::new(),
            media_type: None,
            alternative_id: None,
            any: HashSet::new(),
            group: None,
        }
    }

    /// Try to create a new SOUND property without any parameter or group
    ///
    /// # Errors
    ///   * if given value is not a valid uri value
    pub fn new_validated(value: &str) -> VCardResult<Self> {
        Ok(Self {
            value: Uri::new_validated(value)
                .map_err(VCardError::from_value_error(PropertyKind::Sound))?,
            value_type: None,
            language: None,
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

impl Debug for Sound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sound {{{:?}", self.value)?;
        optional_debug!(self, f, LANG, language);
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

impl TryFrom<&IcalProperty> for Sound {
    type Error = VCardError;

    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Sound));
        };
        let mut result = Self::new_validated(value.as_str())?;
        result.group = group_from_name(&property.name);
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        result.value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::Language => {
                        result.language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::Pid => {
                        result.pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::Pref => {
                        result.preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::Type => {
                        result.r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?;
                    }
                    ParameterType::MediaType => {
                        result.media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::AltId => {
                        result.alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    ParameterType::Any => {
                        result.any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Sound))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Sound,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(result)
    }
}

impl VcardProperty for Sound {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `SOUND` property
///
/// # Errors
///   * the property value is not a valid uri
///   * any of the parameters is invalid
pub fn validate_sound(property: &IcalProperty) -> VcardValidationResult<()> {
    // SOUND-param = "VALUE=uri" / language-param / pid-param / pref-param / type-param / mediatype-param / altid-param / any-param
    // SOUND-value = URI
    if let Some(value) = &property.value {
        if Url::parse(value).is_ok() {
            validate_parameters(
                property,
                ValueType::Uri,
                &hash_set!(
                    ParameterType::Value,
                    ParameterType::Language,
                    ParameterType::Pid,
                    ParameterType::Pref,
                    ParameterType::Type,
                    ParameterType::MediaType,
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
