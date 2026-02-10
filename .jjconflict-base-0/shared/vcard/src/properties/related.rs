use std::collections::HashSet;
use std::fmt::Debug;

use ical::generator::Property as IcalProperty;

use crate::parameters::alternative_id::AlternativeId;
use crate::parameters::any::Any;
use crate::parameters::language::Language;
use crate::parameters::mediatype::MediaType;
use crate::parameters::pid::Pid;
use crate::parameters::preference::Preference;
use crate::parameters::type_related::RelatedType;
use crate::parameters::value::ValueType;
use crate::properties::VcardProperty;
use crate::values::uri::MaybeUri;
use crate::vcard::group_from_name;
use crate::{ParameterType, PropertyKind, VCardError, VCardResult};

/// To specify a relationship between another entity and the entity represented by this vCard.
#[derive(Clone, Default, Debug)]
pub struct Related {
    /// Value (ex: urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6 or Please contact my assistant Jane Doe for any inquiries.)
    pub value: MaybeUri,
    /// type of the value (here nothing or "uri" or "text")
    pub value_type: Option<ValueType>,
    /// Media type linked by the value (only in case of Uri)
    pub media_type: Option<MediaType>,
    /// Language (only with Text)
    pub language: Option<Language>,
    /// The PID parameter is used to identify a specific property among multiple instances.
    pub pid: Option<Pid>,
    /// Preference between other RELATED property
    pub preference: Option<Preference>,
    /// The ALTID parameter is used to "tag" property instances as being alternative representations
    /// of the same logical property.
    pub alternative_id: Option<AlternativeId>,
    /// Type for this property
    pub r#type: HashSet<RelatedType>,
    /// Free parameters
    pub any: HashSet<Any>,
    /// Group this `CalendarUserAddress` belong to
    pub group: Option<String>,
}

impl TryFrom<&IcalProperty> for Related {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(property: &IcalProperty) -> VCardResult<Self> {
        let Some(value) = &property.value else {
            return Err(VCardError::MissingValue(PropertyKind::Related));
        };
        let mut value_type = None;
        let mut pid = None;
        let mut preference = None;
        let mut r#type = HashSet::new();
        let mut media_type = None;
        let mut language = None;
        let mut alternative_id = None;
        let mut any = HashSet::new();
        if let Some(parameters) = &property.params {
            for (name, values) in parameters {
                match ParameterType::from(name.as_str()) {
                    ParameterType::Value => {
                        value_type = Some(
                            ValueType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::Pid => {
                        pid = Some(
                            Pid::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::Pref => {
                        preference = Some(
                            Preference::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::Type => {
                        r#type = RelatedType::set_from_values(values.as_slice())
                            .map_err(VCardError::from_parameter_error(PropertyKind::Related))?;
                    }
                    ParameterType::MediaType => {
                        media_type = Some(
                            MediaType::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::Language => {
                        language = Some(
                            Language::try_from(values.clone())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::AltId => {
                        alternative_id = Some(
                            AlternativeId::try_from(values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    ParameterType::Any => {
                        any.insert(
                            Any::new_validated(name.as_str(), values.as_slice())
                                .map_err(VCardError::from_parameter_error(PropertyKind::Related))?,
                        );
                    }
                    parameter_type => {
                        return Err(VCardError::UnexpectedParameter(
                            PropertyKind::Related,
                            parameter_type,
                        ));
                    }
                }
            }
        }
        Ok(Self {
            value: value.into(),
            value_type,
            pid,
            preference,
            r#type,
            media_type,
            language,
            alternative_id,
            any,
            group: group_from_name(&property.name),
        })
    }
}

impl VcardProperty for Related {
    fn get_preference(&self) -> Option<Preference> {
        self.preference
    }
}
