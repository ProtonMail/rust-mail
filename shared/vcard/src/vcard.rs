//! `VCard` is a structure representing vCards to me manipulated by UI
//!
//! Each property is represented by a dedicated structure and in case of multiple possible value for
//! a property, an enum represent those values. Additionally, the eventual parameters and group are
//! stored in each of those `Property` structures.
//!
//! ## Work in progress
//!
//!   * Some code is already there to handle editing and removing code but the focus was so fare
//!     mainly on exposing the vCard content, so it can be displayed by UI.

// TODO[editing]: sanitize strings on add from UI
//       * trim them (optionally?)
// TODO[editing]: handle change of order (PREF) between properties
// TODO[export]: escape values on export to vCards
// TODO: ALTID parameters should group Property of the same kind together, i.e. they are the same
//       property just with different representation.

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::hash::BuildHasher;

use anyhow::Context;
use ical::generator::VcardContact;
use itertools::Itertools;
use tracing::{error, warn};

use crate::properties::VcardProperty;
use crate::properties::address::Address;
use crate::properties::anniversary::Anniversary;
use crate::properties::birthday::Birthday;
use crate::properties::calendar_uri::CalendarAddress;
use crate::properties::calendar_user_address::CalendarUserAddress;
use crate::properties::categories::Category;
use crate::properties::client_pid_map::ClientPidMap;
use crate::properties::email::Email;
use crate::properties::fburl::FbUrl;
use crate::properties::formatted_name::FormattedName;
use crate::properties::gender::Gender;
use crate::properties::geo::Geo;
use crate::properties::impp::Impp;
use crate::properties::key::Key;
use crate::properties::kind::Kind;
use crate::properties::language::Language;
use crate::properties::logo::Logo;
use crate::properties::member::Member;
use crate::properties::name::Name;
use crate::properties::nickname::Nickname;
use crate::properties::note::Note;
use crate::properties::organization::Organization;
use crate::properties::photo::Photo;
use crate::properties::product_id::ProductId;
use crate::properties::related::Related;
use crate::properties::revision::Revision;
use crate::properties::role::Role;
use crate::properties::sound::Sound;
use crate::properties::source::Source;
use crate::properties::telephone::Telephone;
use crate::properties::time_zone::TimeZone;
use crate::properties::title::Title;
use crate::properties::uid::VcardUid;
use crate::properties::url::VcardUrl;
use crate::properties::xml::Xml;
use crate::properties::xtended::Xtended;
use crate::validation::get_property_kind;
use crate::{PropertyKind, VCardError, VCardResult};

/// Unique identifier for properties inside a vCard
// TODO: Add a custom hasher (No hasher since value is smaller than a hash value)
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PropertyUid(u32);

impl PropertyUid {
    fn increment(&mut self) -> VCardResult<()> {
        self.0 = self
            .0
            .checked_add(1)
            .context("vCard with more than u32::MAX properties are not handled")?;
        Ok(())
    }
}

impl Display for PropertyUid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for PropertyUid {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Representation of a vCard
///
/// `unique_id_counter` is used to give unique id to different properties, so users can edit a
/// specific property.
///
/// `groups` contains all groups from the vCard
#[derive(Debug, Default)]
pub struct VCard {
    pub addresses: HashMap<PropertyUid, Address>,
    pub anniversary: Option<Anniversary>,
    pub birthday: Option<Birthday>,
    pub calendar_addresses: HashMap<PropertyUid, CalendarAddress>,
    pub calendar_user_addresses: HashMap<PropertyUid, CalendarUserAddress>,
    pub categories: HashMap<PropertyUid, Category>,
    pub client_pid_map: HashMap<PropertyUid, ClientPidMap>,
    pub emails: HashMap<PropertyUid, Email>,
    pub fburls: HashMap<PropertyUid, FbUrl>,
    pub formatted_names: HashMap<PropertyUid, FormattedName>,
    pub gender: Option<Gender>,
    pub geos: HashMap<PropertyUid, Geo>,
    pub impps: HashMap<PropertyUid, Impp>,
    pub keys: HashMap<PropertyUid, Key>,
    pub kind: Option<Kind>,
    pub languages: HashMap<PropertyUid, Language>,
    pub logos: HashMap<PropertyUid, Logo>,
    pub members: HashMap<PropertyUid, Member>,
    pub name: Option<Name>,
    pub nicknames: HashMap<PropertyUid, Nickname>,
    pub notes: HashMap<PropertyUid, Note>,
    pub organizations: HashMap<PropertyUid, Organization>,
    pub photos: HashMap<PropertyUid, Photo>,
    pub product_id: Option<ProductId>,
    pub related: HashMap<PropertyUid, Related>,
    pub revision: Option<Revision>,
    pub roles: HashMap<PropertyUid, Role>,
    pub sounds: HashMap<PropertyUid, Sound>,
    pub sources: HashMap<PropertyUid, Source>,
    pub telephones: HashMap<PropertyUid, Telephone>,
    pub time_zones: HashMap<PropertyUid, TimeZone>,
    pub titles: HashMap<PropertyUid, Title>,
    pub uid: Option<VcardUid>,
    pub urls: HashMap<PropertyUid, VcardUrl>,
    pub xmls: HashMap<PropertyUid, Xml>,
    pub xtendeds: HashMap<PropertyUid, Xtended>,
    /// to generate unique ids for properties
    unique_id_counter: PropertyUid,
    /// associate group from property name (key) with corresponding category value
    groups: HashMap<String, String>,
}

impl VCard {
    #[tracing::instrument(skip_all)]
    #[allow(clippy::too_many_lines)]
    pub fn from_ical_contact(value: VcardContact) -> VCardResult<Self> {
        let mut result = VCard::new();

        match value.properties.first() {
            Some(v) if v.value.as_deref() == Some("VERSION") => (),
            _ => {
                warn!("Vcard error: Missing version");
            }
        }

        for property in value.properties {
            if let Err(err) = (|| {
                let Some(value) = &property.value else {
                    match get_property_kind(&property.name)
                        .map_err(|_| VCardError::InvalidPropertyName(property.name.clone()))?
                    {
                        // Only property where no value is possible (with Ical crate)
                        PropertyKind::Gender => result.set_gender(Gender::try_from(&property)?),
                        PropertyKind::Extended(_) => {
                            result.add_xtended(Xtended::try_from(&property)?)?;
                        }
                        property_kind => return Err(VCardError::MissingValue(property_kind)),
                    }
                    return Ok(());
                };

                match get_property_kind(&property.name)
                    .map_err(|_| VCardError::InvalidPropertyName(property.name.clone()))?
                {
                    PropertyKind::Begin | PropertyKind::End => (),
                    PropertyKind::Adr => {
                        result.add_address(Address::try_from(property)?)?;
                    }
                    PropertyKind::Anniversary => {
                        result.set_anniversary(Anniversary::try_from(&property)?);
                    }
                    PropertyKind::BDay => result.set_birthday(Birthday::try_from(&property)?),
                    PropertyKind::CalAdrURI => {
                        result
                            .add_calendar_user_address(CalendarUserAddress::try_from(&property)?)?;
                    }
                    PropertyKind::CalURI => {
                        result.add_calendar_address(CalendarAddress::try_from(&property)?)?;
                    }
                    PropertyKind::Categories => {
                        result.add_category(Category::try_from(&property)?)?;
                        if let Some((id, _)) = property.name.split_once('.')
                            && result.add_group(id.to_owned(), value.to_owned()).is_some()
                        {
                            warn!("Two CATEGORIES property are in the same group ({id})");
                        }
                    }
                    PropertyKind::ClientPIDMap => {
                        result.add_client_pid_map(ClientPidMap::try_from(&property)?)?;
                    }
                    PropertyKind::Email => {
                        result.add_email(Email::try_from(&property)?)?;
                    }
                    PropertyKind::FbUrl => {
                        result.add_fburl(FbUrl::try_from(&property)?)?;
                    }
                    PropertyKind::Fn => {
                        result.add_formatted_name(FormattedName::try_from(&property)?)?;
                    }
                    PropertyKind::Gender => result.set_gender(Gender::try_from(&property)?),
                    PropertyKind::Geo => {
                        result.add_geo(Geo::try_from(&property)?)?;
                    }
                    PropertyKind::Impp => {
                        result.add_impp(Impp::try_from(&property)?)?;
                    }
                    PropertyKind::Key => {
                        result.add_key(Key::try_from(&property)?)?;
                    }
                    PropertyKind::Kind => result.set_kind(Kind::try_from(&property)?),
                    PropertyKind::Lang => {
                        result.add_language(Language::try_from(&property)?)?;
                    }
                    PropertyKind::Logo => {
                        result.add_logo(Logo::try_from(&property)?)?;
                    }
                    PropertyKind::Member => {
                        result.add_member(Member::try_from(property)?)?;
                    }
                    PropertyKind::N => result.set_name(Name::try_from(&property)?),
                    PropertyKind::Nickname => {
                        result.add_nickname(Nickname::try_from(&property)?)?;
                    }
                    PropertyKind::Note => {
                        result.add_note(Note::try_from(&property)?)?;
                    }
                    PropertyKind::Org => {
                        result.add_organization(Organization::try_from(&property)?)?;
                    }
                    PropertyKind::Photo => {
                        result.add_photo(Photo::try_from(&property)?)?;
                    }
                    PropertyKind::ProdId => result.set_product_id(ProductId::try_from(property)?),
                    PropertyKind::Related => {
                        result.add_related(Related::try_from(&property)?)?;
                    }
                    PropertyKind::Rev => result.set_revision(Revision::try_from(&property)?),
                    PropertyKind::Role => {
                        result.add_role(Role::try_from(&property)?)?;
                    }
                    PropertyKind::Sound => {
                        result.add_sound(Sound::try_from(&property)?)?;
                    }
                    PropertyKind::Source => {
                        result.add_source(Source::try_from(&property)?)?;
                    }
                    PropertyKind::Tel => {
                        result.add_telephone(Telephone::try_from(&property)?)?;
                    }
                    PropertyKind::Title => {
                        result.add_title(Title::try_from(&property)?)?;
                    }
                    PropertyKind::Tz => {
                        result.add_time_zone(TimeZone::try_from(&property)?)?;
                    }
                    PropertyKind::UId => result.set_uid(VcardUid::try_from(&property)?),
                    PropertyKind::Url => {
                        result.add_url(VcardUrl::try_from(property)?)?;
                    }
                    PropertyKind::Xml => {
                        result.add_xml(Xml::try_from(&property)?)?;
                    }
                    PropertyKind::Extended(_) => {
                        result.add_xtended(Xtended::try_from(&property)?)?;
                    }
                    PropertyKind::Version => {
                        warn!("Unsupported version, will try to keep parsing anyways");
                    }
                }
                Ok(())
            })() {
                error!("Vcard error parsing property: {err:?}");
            }
        }
        Ok(result)
    }
}

/// Macro to display an optional property if present
macro_rules! display_optional {
    ($self:ident, $f:ident, $name:ident) => {
        if let Some(v) = &$self.$name {
            writeln!($f, "  {}: {v:?}", stringify!($name))?;
        }
    };
}

/// Macro to display a set of properties if any is present
macro_rules! display_set {
    ($self:ident, $f:ident, $name:ident) => {
        if !$self.$name.is_empty() {
            writeln!($f, "  {}:", stringify!($name))?;
            for (id, value) in &$self.$name {
                writeln!($f, "    {id} -> {value:?}")?;
            }
        }
    };
}

impl Display for VCard {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "VCard:")?;
        if !self.groups.is_empty() {
            writeln!(f, "  groups:")?;
            for group in &self.groups {
                writeln!(f, "    {group:?}")?;
            }
            writeln!(f)?;
        }
        display_set!(self, f, addresses);
        display_optional!(self, f, anniversary);
        display_optional!(self, f, birthday);
        display_set!(self, f, calendar_addresses);
        display_set!(self, f, calendar_user_addresses);
        display_set!(self, f, categories);
        display_set!(self, f, client_pid_map);
        display_set!(self, f, emails);
        display_set!(self, f, fburls);
        display_set!(self, f, formatted_names);
        display_optional!(self, f, gender);
        display_set!(self, f, geos);
        display_set!(self, f, impps);
        display_set!(self, f, keys);
        display_optional!(self, f, kind);
        display_set!(self, f, languages);
        display_set!(self, f, logos);
        display_set!(self, f, members);
        display_optional!(self, f, name);
        display_set!(self, f, nicknames);
        display_set!(self, f, notes);
        display_set!(self, f, organizations);
        display_set!(self, f, photos);
        display_optional!(self, f, product_id);
        display_set!(self, f, related);
        display_optional!(self, f, revision);
        display_set!(self, f, roles);
        display_set!(self, f, sounds);
        display_set!(self, f, sources);
        display_set!(self, f, telephones);
        display_set!(self, f, titles);
        display_set!(self, f, time_zones);
        display_optional!(self, f, uid);
        display_set!(self, f, urls);
        display_set!(self, f, xmls);
        display_set!(self, f, xtendeds);
        Ok(())
    }
}

impl TryFrom<VcardContact> for VCard {
    type Error = VCardError;

    #[allow(clippy::too_many_lines)]
    fn try_from(value: VcardContact) -> VCardResult<Self> {
        Self::from_ical_contact(value)
    }
}

macro_rules! optional_handler {
    ($name:ident, $type:ty) => {
        paste::paste! {
            #[doc = "Set "]
            #[doc = stringify!($name)]
            #[doc = " value, if any value was there it's replaced without warning"]
            pub fn [<set_ $name>](&mut self, value: $type) {
                if self.$name.is_some() {
                    warn!("Overwriting {}", stringify!($name));
                }
                self.$name = Some(value);
            }

            #[doc = "Remove the value from "]
            #[doc = stringify!($name)]
            #[doc = " if any"]
            pub fn [<unset_ $name>](&mut self) {
                self.$name = None;
            }
        }
    };
}

macro_rules! set_handler {
    ($name:ident, $plural:ident, $type:ty) => {
        paste::paste! {
            #[doc = "Add a "]
            #[doc = stringify!($name)]
            #[doc = " property to the vCard"]
            ///
            /// # Errors
            ///   * too many property in vCard
            pub fn [<add_ $name>](&mut self, value: $type) -> VCardResult<PropertyUid> {
                let uid = self.next_uid()?;
                self.$plural.insert(uid, value);
                Ok(uid)
            }

            #[doc = "Get "]
            #[doc = stringify!($name)]
            #[doc = " property if uid exist for this property"]
            #[must_use]
            pub fn [<get_ $name>](&self, uid: PropertyUid) -> Option<$type> {
                self.$plural.get(&uid).cloned()
            }

            #[doc = "Get the preferred "]
            #[doc = stringify!($name)]
            #[doc = " if any with its uid"]
            #[must_use] pub fn [<get_preferred_ $name>](&self) -> Option<(PropertyUid, $type)> {
                get_preferred(&self.$plural)
            }

            #[doc = "Get all "]
            #[doc = stringify!($name)]
            #[doc = " properties with their uid"]
            #[must_use] pub fn [<get_all_ $name>](&self) -> Vec<(PropertyUid, $type)> {
                self.$plural
                    .iter()
                    .map(|(&k, v)| (k, v.clone()))
                    .collect()
            }

            #[doc = "Get all "]
            #[doc = stringify!($name)]
            #[doc = " properties without their uid"]
            #[must_use] pub fn [<get_all_ $name _plain>](&self) -> Vec<$type> {
                self.$plural
                    .iter()
                    .map(|(_, v)| v.clone())
                    .collect()
            }

            #[doc = "Remove the "]
            #[doc = stringify!($name)]
            #[doc = " property with uid (return tell if that property existed)"]
            pub fn [<remove_ $name>](&mut self, uid: PropertyUid) -> bool {
                self.$plural.remove(&uid).is_some()
            }
        }
    };
}

impl VCard {
    /// Create a new empty vCard
    fn new() -> Self {
        Self::default()
    }

    /// Add a group into the vCard
    pub fn add_group(&mut self, id: String, label: String) -> Option<String> {
        self.groups.insert(id, label)
    }

    /// Get all groups from vCard
    // TODO: create struct for group to replace this tuple
    #[must_use]
    pub fn get_all_groups(&self) -> Vec<(&str, &str)> {
        self.groups
            .iter()
            .map(|(i, n)| (i.as_str(), n.as_str()))
            .collect()
    }

    /// Get the label corresponding to a group
    #[must_use]
    pub fn get_group_label(&self, group: &str) -> Option<&str> {
        self.groups.get(group).map(String::as_str)
    }

    set_handler!(address, addresses, Address);
    optional_handler!(anniversary, Anniversary);
    optional_handler!(birthday, Birthday);
    set_handler!(calendar_address, calendar_addresses, CalendarAddress);
    set_handler!(
        calendar_user_address,
        calendar_user_addresses,
        CalendarUserAddress
    );
    set_handler!(category, categories, Category);
    set_handler!(client_pid_map, client_pid_map, ClientPidMap);
    set_handler!(email, emails, Email);
    set_handler!(fburl, fburls, FbUrl);
    set_handler!(formatted_name, formatted_names, FormattedName);
    optional_handler!(gender, Gender);
    set_handler!(geo, geos, Geo);
    set_handler!(impp, impps, Impp);
    set_handler!(key, keys, Key);
    optional_handler!(kind, Kind);
    set_handler!(language, languages, Language);
    set_handler!(logo, logos, Logo);
    set_handler!(member, members, Member);
    optional_handler!(name, Name);
    set_handler!(note, notes, Note);
    set_handler!(nickname, nicknames, Nickname);
    set_handler!(organization, organizations, Organization);
    set_handler!(photo, photos, Photo);
    optional_handler!(product_id, ProductId);
    set_handler!(related, related, Related);
    optional_handler!(revision, Revision);
    set_handler!(role, roles, Role);
    set_handler!(sound, sounds, Sound);
    set_handler!(source, sources, Source);
    set_handler!(telephone, telephones, Telephone);
    set_handler!(title, titles, Title);
    set_handler!(time_zone, time_zones, TimeZone);
    optional_handler!(uid, VcardUid);
    set_handler!(url, urls, VcardUrl);
    set_handler!(xml, xmls, Xml);
    set_handler!(xtended, xtendeds, Xtended);

    // Utility
    // ---------------------------------------------------------------------------------------------
    /// Get next unique id
    fn next_uid(&mut self) -> VCardResult<PropertyUid> {
        self.unique_id_counter.increment()?;
        Ok(self.unique_id_counter)
    }
}

/// Lookup in a set of property and return the preferred
/// Note: In case of identical preference, the result is undetermined
/// Note: The lower preference value are preferred (no value -> worst)
#[must_use]
pub fn get_preferred<T: VcardProperty + Clone, S: BuildHasher>(
    values: &HashMap<PropertyUid, T, S>,
) -> Option<(PropertyUid, T)> {
    let mut result = None;
    let mut best = None;
    for (id, value) in values {
        match (best, value.get_preference()) {
            (None, Some(p)) => {
                result = Some((*id, value));
                best = Some(p);
            }
            (Some(b), Some(p)) if p.value < b.value => {
                result = Some((*id, value));
                best = Some(p);
            }
            _ => (),
        }
    }
    result.map(|(i, v)| (i, v.clone()))
}

/// Split a string encoding a list while handling escaped separator
pub(crate) fn split_list(value: &str, separator: char) -> Vec<String> {
    let mut offset = 0;
    let mut start = 0;
    let mut result = vec![];
    while let Some(position) = value[offset..].find(separator) {
        offset += position + 1;
        if offset < 2 {
            // value start with a comma
            result.push(String::new());
            start = offset;
        } else if value.get(offset - 2..offset - 1) != Some(r"\") {
            result.push(value[start..offset - 1].to_owned());
            start = offset;
        }
    }
    result.push(value[start..].to_owned());
    result
}

/// Get the group part of a property name if any.
pub(crate) fn group_from_name(name: &str) -> Option<String> {
    name.split_once('.').map(|(g, _)| g.to_owned())
}

/// This trait exists solely for convenience, to transform the fields in the vcard into others in an efficient manner.
pub trait ToSorted<P> {
    fn to_sorted_iter<T: Ord>(self, f: impl FnMut(P) -> T) -> impl Iterator<Item = T>
    where
        Self: Sized;

    /// Convenience method that extends a vector.
    /// f1 is typically an enum variant f2 is a map method
    fn sorted_extend<T: Ord, U>(
        self,
        vec: &mut Vec<U>,
        mut f1: impl FnMut(Vec<T>) -> U,
        f2: impl FnMut(P) -> T,
    ) where
        Self: Sized,
    {
        let vals = self.to_sorted_iter(f2).collect_vec();
        if !vals.is_empty() {
            vec.push(f1(vals));
        }
    }
}

impl<_K, P: VcardProperty, S: BuildHasher> ToSorted<P> for HashMap<_K, P, S> {
    fn to_sorted_iter<T: Ord>(self, mut f: impl FnMut(P) -> T) -> impl Iterator<Item = T>
    where
        Self: Sized,
    {
        self.into_values()
            .map(|this| {
                let pref = match this.get_preference() {
                    Some(v) => v.value,
                    None => u32::MAX,
                };
                (pref, f(this))
            })
            .sorted_unstable()
            .map(|x| x.1)
    }
}
