//! This module regroup all the logic around vCard properties

pub mod address;
pub mod anniversary;
pub mod begin;
pub mod birthday;
pub mod calendar_uri;
pub mod calendar_user_address;
pub mod categories;
pub mod client_pid_map;
pub mod email;
pub mod end;
pub mod fburl;
pub mod formatted_name;
pub mod gender;
pub mod geo;
pub mod impp;
pub mod key;
pub mod kind;
pub mod language;
pub mod logo;
pub mod member;
pub mod name;
pub mod nickname;
pub mod note;
pub mod organization;
pub mod photo;
pub mod product_id;
pub mod related;
pub mod revision;
pub mod role;
pub mod sound;
pub mod source;
pub mod telephone;
pub mod time_zone;
pub mod title;
pub mod uid;
pub mod url;
pub mod version;
pub mod xml;
pub mod xtended;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::ParameterType;
use crate::parameters::alternative_id::is_altid_param;
use crate::parameters::any::is_any_param;
use crate::parameters::calendar_scale::is_calscale_param;
use crate::parameters::geo_localisation::is_geo_param;
use crate::parameters::label::is_label_param;
use crate::parameters::language::is_language_param;
use crate::parameters::mediatype::is_mediatype_param;
use crate::parameters::pid::is_pid_param;
use crate::parameters::preference::{Preference, is_pref_param};
use crate::parameters::sort_as::is_sort_as_param;
use crate::parameters::time_zone::is_tz_param;
use crate::parameters::type_generic::is_type_param;
use crate::parameters::value::ValueType;
use crate::parameters::value::is_value_param;
use crate::validation::get_property_kind;
use ical::property::Property;
use std::collections::HashSet;

/// Trait to define a vCard property
pub trait VcardProperty {
    /// Get the preference parameter of a property if any
    fn get_preference(&self) -> Option<Preference>;
}

/// All possible properties in a vCard
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PropertyKind {
    /// Address
    Adr,
    /// Anniversary
    Anniversary,
    /// Birthday
    BDay,
    /// vCard begin
    Begin,
    /// URI to the calendar user
    CalAdrURI,
    /// URI to the calendar associated with current vCard
    CalURI,
    /// Tags for current vCard
    Categories,
    /// Map between PID
    ClientPIDMap,
    /// Email address
    Email,
    /// vCard end
    End,
    /// Busy Time
    FbUrl,
    /// Formatted Name
    Fn,
    /// Gender
    Gender,
    /// Global positioning for vCard object
    Geo,
    /// Instant messaging and presence protocol
    Impp,
    /// Public key or authentication certificate
    Key,
    /// Kind of object for the vCard
    Kind,
    /// Language to use with vCard object
    Lang,
    /// Image logo representing vCard object
    Logo,
    /// Group where belong the vCard object
    Member,
    /// Name of vCard object
    N,
    /// Nickname
    Nickname,
    /// Additional information about vCard object
    Note,
    /// Organisation where vCard object belong
    Org,
    /// Photo representing vCard object
    Photo,
    /// Identifier to the vCard producer
    ProdId,
    /// Define a relation between vCard object and another entity
    Related,
    /// vCard revision
    Rev,
    /// Function or part played by vCard object
    Role,
    /// Sound associated with vCard object (ex: pronunciation of name property)
    Sound,
    /// Source of directory information about vCard object
    Source,
    /// Telephone number
    Tel,
    /// Position of vCard object
    Title,
    /// Time zone
    Tz,
    /// Globally unique identifier for vCard object
    UId,
    /// URL associated with vCard object
    Url,
    /// Version of vCard specification
    Version,
    /// Extended XML-encoded vCard
    Xml,
    /// Additional Property
    Extended(String),
}

impl TryFrom<&str> for PropertyKind {
    type Error = VcardValidationError;

    fn try_from(value: &str) -> VcardValidationResult<Self> {
        match value.to_ascii_uppercase().as_str() {
            "ADR" => Ok(PropertyKind::Adr),
            "ANNIVERSARY" => Ok(PropertyKind::Anniversary),
            "BDAY" => Ok(PropertyKind::BDay),
            "BEGIN" => Ok(PropertyKind::Begin),
            "CALADRURI" => Ok(PropertyKind::CalAdrURI),
            "CALURI" => Ok(PropertyKind::CalURI),
            "CATEGORIES" => Ok(PropertyKind::Categories),
            "CLIENTPIDMAP" => Ok(PropertyKind::ClientPIDMap),
            "EMAIL" => Ok(PropertyKind::Email),
            "END" => Ok(PropertyKind::End),
            "FBURL" => Ok(PropertyKind::FbUrl),
            "FN" => Ok(PropertyKind::Fn),
            "GENDER" => Ok(PropertyKind::Gender),
            "GEO" => Ok(PropertyKind::Geo),
            "IMPP" => Ok(PropertyKind::Impp),
            "KEY" => Ok(PropertyKind::Key),
            "KIND" => Ok(PropertyKind::Kind),
            "LANG" => Ok(PropertyKind::Lang),
            "LOGO" => Ok(PropertyKind::Logo),
            "MEMBER" => Ok(PropertyKind::Member),
            "N" => Ok(PropertyKind::N),
            "NICKNAME" => Ok(PropertyKind::Nickname),
            "NOTE" => Ok(PropertyKind::Note),
            "ORG" => Ok(PropertyKind::Org),
            "PHOTO" => Ok(PropertyKind::Photo),
            "PRODID" => Ok(PropertyKind::ProdId),
            "RELATED" => Ok(PropertyKind::Related),
            "REV" => Ok(PropertyKind::Rev),
            "ROLE" => Ok(PropertyKind::Role),
            "SOUND" => Ok(PropertyKind::Sound),
            "SOURCE" => Ok(PropertyKind::Source),
            "TEL" => Ok(PropertyKind::Tel),
            "TITLE" => Ok(PropertyKind::Title),
            "TZ" => Ok(PropertyKind::Tz),
            "UID" => Ok(PropertyKind::UId),
            "URL" => Ok(PropertyKind::Url),
            "VERSION" => Ok(PropertyKind::Version),
            "XML" => Ok(PropertyKind::Xml),
            extended if extended.starts_with("X-") => Ok(PropertyKind::Extended(value.to_owned())),
            _ => Err(VcardValidationError::InvalidPropertyName(value.to_owned())),
        }
    }
}

impl PartialEq<String> for PropertyKind {
    fn eq(&self, other: &String) -> bool {
        let other = other.to_ascii_uppercase();
        match self {
            PropertyKind::Adr => other == "ADR",
            PropertyKind::Anniversary => other == "ANNIVERSARY",
            PropertyKind::BDay => other == "BDAY",
            PropertyKind::Begin => other == "BEGIN",
            PropertyKind::CalAdrURI => other == "CALADRURI",
            PropertyKind::CalURI => other == "CALURI",
            PropertyKind::Categories => other == "CATEGORIES",
            PropertyKind::ClientPIDMap => other == "CLIENTPIDMAP",
            PropertyKind::Email => other == "EMAIL",
            PropertyKind::End => other == "END",
            PropertyKind::FbUrl => other == "FBURL",
            PropertyKind::Fn => other == "FN",
            PropertyKind::Gender => other == "GENDER",
            PropertyKind::Geo => other == "GEO",
            PropertyKind::Impp => other == "IMPP",
            PropertyKind::Key => other == "KEY",
            PropertyKind::Kind => other == "KIND",
            PropertyKind::Lang => other == "LANG",
            PropertyKind::Logo => other == "LOGO",
            PropertyKind::Member => other == "MEMBER",
            PropertyKind::N => other == "N",
            PropertyKind::Nickname => other == "NICKNAME",
            PropertyKind::Note => other == "Note",
            PropertyKind::Org => other == "ORG",
            PropertyKind::Photo => other == "PHOTO",
            PropertyKind::ProdId => other == "PRODID",
            PropertyKind::Related => other == "RELATED",
            PropertyKind::Rev => other == "REV",
            PropertyKind::Role => other == "ROLE",
            PropertyKind::Sound => other == "SOUND",
            PropertyKind::Source => other == "SOURCE",
            PropertyKind::Tel => other == "TEL",
            PropertyKind::Title => other == "TITLE",
            PropertyKind::Tz => other == "TZ",
            PropertyKind::UId => other == "UID",
            PropertyKind::Url => other == "URL",
            PropertyKind::Version => other == "VERSION",
            PropertyKind::Xml => other == "XML",
            PropertyKind::Extended(value) => &other == value,
        }
    }
}

/// Validate that all parameters of the given `property` are valid and in the allowed list for that `property`
///
/// # Errors
///   * at least one of the parameters is invalid
///   * at least one of the parameters is not authorized
pub fn validate_parameters<S: ::std::hash::BuildHasher>(
    property: &Property,
    value_type: ValueType,
    allowed: &HashSet<ParameterType, S>,
) -> VcardValidationResult<()> {
    if let Some(params) = &property.params {
        for (name, values) in params {
            let param_type = ParameterType::from(name.as_str());
            if allowed.contains(&param_type) {
                let validate = match param_type {
                    ParameterType::Value => is_value_param(values, value_type),
                    ParameterType::AltId => is_altid_param(values),
                    ParameterType::CalScale => is_calscale_param(values),
                    ParameterType::Geo => is_geo_param(values),
                    ParameterType::Label => is_label_param(values),
                    ParameterType::Language => is_language_param(values),
                    ParameterType::MediaType => is_mediatype_param(values),
                    ParameterType::Pid => is_pid_param(values),
                    ParameterType::Pref => is_pref_param(values),
                    ParameterType::SortAs => is_sort_as_param(values),
                    ParameterType::Type => {
                        let property = get_property_kind(&property.name)?;
                        is_type_param(&property, values)
                    }
                    ParameterType::TZ => is_tz_param(values),
                    ParameterType::Any => is_any_param(name, values),
                };
                if !validate {
                    return Err(VcardValidationError::InvalidPropertyParam(
                        get_property_kind(&property.name)?,
                        name.to_owned(),
                    ));
                }
            } else {
                return Err(VcardValidationError::UnexpectedPropertyParam(
                    get_property_kind(&property.name)?,
                    name.to_owned(),
                ));
            }
        }
    }
    Ok(())
}

/// Get the value type from VALUE parameter if any
fn get_value_type(property: &Property) -> VcardValidationResult<Option<ValueType>> {
    if let Some(params) = &property.params {
        for (name, values) in params {
            if name.eq_ignore_ascii_case("VALUE") {
                return if values.len() == 1 {
                    if let Ok(value) = ValueType::try_from(values[0].as_str()) {
                        Ok(Some(value))
                    } else {
                        Err(VcardValidationError::InvalidPropertyParam(
                            get_property_kind(property.name.as_str())?,
                            name.to_owned(),
                        ))
                    }
                } else {
                    Err(VcardValidationError::InvalidPropertyParam(
                        get_property_kind(property.name.as_str())?,
                        name.to_owned(),
                    ))
                };
            }
        }
    }
    Ok(None)
}
