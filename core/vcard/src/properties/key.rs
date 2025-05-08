use ical::generator::Property as IcalProperty;
use std::collections::HashSet;
use std::fmt::Debug;
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
use crate::properties::{VcardProperty, get_value_type, validate_parameters};
use crate::validation::get_property_kind;
use crate::values::uri::MaybeUri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify a public key or authentication certificate associated with the object that the vCard
/// represents.
#[derive(Clone, Debug, Default)]
pub struct Key {
    /// Value (ex: (text) <ftp://example.com/keys/jdoe>, (uri) <data:application/pgp-keys;base64,MIICajCCAdOgAwIBAgICBEUw...>)
    pub value: MaybeUri,
    /// type of the value (here nothing or "uri" or "text")
    pub value_type: Option<ValueType>,
    /// Media type linked by the value
    pub media_type: Option<MediaType>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other CALADRURI property
    pub preference: Option<Preference>,
    /// Type for this property
    pub r#type: HashSet<GenericType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl TryFrom<&IcalProperty> for Key {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Key));
        };
        let mut value_type = None;
        let mut pid = None;
        let mut preference = None;
        let mut r#type = HashSet::new();
        let mut media_type = None;
        let mut alternative_id = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    ParameterType::MediaType => {
                        media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    ParameterType::Pid => {
                        pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    ParameterType::Pref => {
                        preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    ParameterType::Type => {
                        r#type = GenericType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Key))?;
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Key))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Key,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(Self {
            value: value.into(),
            value_type,
            media_type,
            alternative_id,
            pid,
            preference,
            r#type,
            any,
            group: group_from_name(&property.name),
        })
    }
}

impl VcardProperty for Key {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}

/// Validate that the given `property` respect the format for a `KEY` property
///
/// # Errors
///   * if property value is not a valid text value or uri value
///   * if any parameter is not valid
pub fn validate_key(property: &IcalProperty) -> VcardValidationResult<()> {
    // KEY-param = KEY-uri-param / KEY-text-param
    // KEY-value = KEY-uri-value / KEY-text-value
    //   ; Value and parameter MUST match.
    //
    // KEY-uri-param = "VALUE=uri" / mediatype-param
    // KEY-uri-value = URI
    //
    // KEY-text-param = "VALUE=text"
    // KEY-text-value = text
    //
    // KEY-param =/ altid-param / pid-param / pref-param / type-param / any-param
    if let Some(value) = &property.value {
        let value_type = if let Some(value_type) = get_value_type(property)? {
            match value_type {
                ValueType::Text => true,
                ValueType::Uri => Url::parse(value).is_ok(),
                _ => {
                    return Err(VcardValidationError::InvalidPropertyValue(
                        get_property_kind(&property.name)?,
                    ));
                }
            };
            value_type
        } else {
            ValueType::Text
        };
        let allowed = if matches!(value_type, ValueType::Text) {
            hash_set!(
                ParameterType::Value,
                ParameterType::AltId,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::Type,
                ParameterType::Any
            )
        } else {
            hash_set!(
                ParameterType::Value,
                ParameterType::MediaType,
                ParameterType::AltId,
                ParameterType::Pid,
                ParameterType::Pref,
                ParameterType::Type,
                ParameterType::Any
            )
        };
        validate_parameters(property, value_type, &allowed)?;
    } else {
        return Err(VcardValidationError::InvalidPropertyValue(
            get_property_kind(&property.name)?,
        ));
    }
    Ok(())
}
