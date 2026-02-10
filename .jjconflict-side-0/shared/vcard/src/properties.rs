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
pub mod xml;
pub mod xtended;

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::parameters::preference::Preference;

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
            _name => Err(VcardValidationError::InvalidPropertyName(value.to_owned())),
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
