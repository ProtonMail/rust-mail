//! This module group all higher level validation functions

use crate::errors::{VcardValidationError, VcardValidationResult};
use crate::properties::PropertyKind;
use crate::properties::address::validate_adr;
use crate::properties::anniversary::validate_anniversary;
use crate::properties::begin::validate_begin;
use crate::properties::birthday::validate_bday;
use crate::properties::calendar_uri::validate_caluri;
use crate::properties::calendar_user_address::validate_caladruri;
use crate::properties::categories::validate_categories;
use crate::properties::client_pid_map::validate_clientpidmap;
use crate::properties::email::validate_email;
use crate::properties::end::validate_end;
use crate::properties::fburl::validate_fburl;
use crate::properties::formatted_name::validate_fn;
use crate::properties::gender::validate_gender;
use crate::properties::geo::validate_geo;
use crate::properties::impp::validate_impp;
use crate::properties::key::validate_key;
use crate::properties::kind::validate_kind;
use crate::properties::language::validate_lang;
use crate::properties::logo::validate_logo;
use crate::properties::member::validate_member;
use crate::properties::name::validate_n;
use crate::properties::nickname::validate_nickname;
use crate::properties::note::validate_note;
use crate::properties::organization::validate_org;
use crate::properties::photo::validate_photo;
use crate::properties::product_id::validate_prodid;
use crate::properties::related::validate_related;
use crate::properties::revision::validate_rev;
use crate::properties::role::validate_role;
use crate::properties::sound::validate_sound;
use crate::properties::source::validate_source;
use crate::properties::telephone::validate_tel;
use crate::properties::time_zone::validate_tz;
use crate::properties::title::validate_title;
use crate::properties::uid::validate_uid;
use crate::properties::url::validate_url;
use crate::properties::version::validate_version;
use crate::properties::xml::validate_xml;
use ical::VcardParser;
use ical::generator::{Property, VcardContact};
use regex::Regex;
use std::collections::HashMap;
use std::io::BufRead;
use velcro::hash_map;

/// Represent the cardinality for properties
#[derive(Debug)]
pub enum Cardinality {
    /// 1*
    OneOrMoreMust,
    /// 1
    ExactlyOneMust,
    /// *1
    ExactlyOneMay,
    /// *
    OneOrMoreMay,
}

/// Validate a vCard
///
/// # Errors
/// * At least one of the contact in vCard is invalid
pub fn validate_vcard(card: impl BufRead) -> VcardValidationResult<()> {
    let card = VcardParser::new(card);
    for contact in card {
        validate_contact(&contact?)?;
    }
    Ok(())
}

/// Validate a contact from vCard
///
/// # Errors
/// * The order of the property in incorrect
/// * At least one of the property is invalid
fn validate_contact(contact: &VcardContact) -> VcardValidationResult<()> {
    validate_contact_order(contact)?;
    validate_contact_cardinality(contact)?;
    for property in &contact.properties {
        validate_property(property)?;
    }
    Ok(())
}

/// Validate cardinality of the properties in a contact
///
/// # Errors
/// * At least property have its cardinality wrong
fn validate_contact_cardinality(contact: &VcardContact) -> VcardValidationResult<()> {
    fn validate_property_cardinality(
        property: &PropertyKind,
        count: usize,
    ) -> VcardValidationResult<()> {
        match property {
            // Exactly one must be present
            PropertyKind::Begin | PropertyKind::End | PropertyKind::Version => {
                if count == 1 {
                    Ok(())
                } else {
                    Err(VcardValidationError::InvalidPropertiesCardinality(
                        property.clone(),
                        Cardinality::ExactlyOneMust,
                    ))
                }
            }
            // Exactly one may be present
            PropertyKind::Kind
            | PropertyKind::N
            | PropertyKind::BDay
            | PropertyKind::Anniversary
            | PropertyKind::Gender
            | PropertyKind::ProdId
            | PropertyKind::Rev
            | PropertyKind::UId => {
                if count > 1 {
                    Err(VcardValidationError::InvalidPropertiesCardinality(
                        property.clone(),
                        Cardinality::ExactlyOneMay,
                    ))
                } else {
                    Ok(())
                }
            }
            PropertyKind::Fn => {
                if count < 1 {
                    Err(VcardValidationError::InvalidPropertiesCardinality(
                        property.clone(),
                        Cardinality::OneOrMoreMust,
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    fn get_altid(property: &Property) -> Option<String> {
        property.params.as_ref().and_then(|p| {
            p.iter().find_map(|(n, v)| {
                if n.eq_ignore_ascii_case("ALTID") && !v.is_empty() {
                    Some(v[0].clone())
                } else {
                    None
                }
            })
        })
    }

    // Property with the same ALTID parameter are considered as the same.
    let mut counters = HashMap::new();
    for property in &contact.properties {
        let property_kind = get_property_kind(&property.name)?;
        let inner = counters.entry(property_kind).or_insert(hash_map! {});
        *inner.entry(get_altid(property)).or_insert(0_usize) += 1;
    }
    if !counters.contains_key(&PropertyKind::Fn) {
        return Err(VcardValidationError::InvalidPropertiesCardinality(
            PropertyKind::Fn,
            Cardinality::OneOrMoreMust,
        ));
    }
    // BEGIN and END are handled and removed by `iCal`
    if !counters.contains_key(&PropertyKind::Version) {
        return Err(VcardValidationError::InvalidPropertiesCardinality(
            PropertyKind::Version,
            Cardinality::ExactlyOneMust,
        ));
    }
    for (property, values) in &counters {
        // All different altid + all without altid - 1 for none
        let count = values.len() + values.get(&None).unwrap_or(&0) - 1;
        validate_property_cardinality(property, count)?;
    }
    Ok(())
}

/// Validate that the given `property` is valid for the given `kind`
fn do_validate_property(kind: &PropertyKind, property: &Property) -> VcardValidationResult<()> {
    match kind {
        PropertyKind::Adr => validate_adr(property),
        PropertyKind::Anniversary => validate_anniversary(property),
        PropertyKind::BDay => validate_bday(property),
        PropertyKind::Begin => validate_begin(property),
        PropertyKind::CalAdrURI => validate_caladruri(property),
        PropertyKind::CalURI => validate_caluri(property),
        PropertyKind::Categories => validate_categories(property),
        PropertyKind::ClientPIDMap => validate_clientpidmap(property),
        PropertyKind::Email => validate_email(property),
        PropertyKind::End => validate_end(property),
        PropertyKind::FbUrl => validate_fburl(property),
        PropertyKind::Fn => validate_fn(property),
        PropertyKind::Gender => validate_gender(property),
        PropertyKind::Geo => validate_geo(property),
        PropertyKind::Impp => validate_impp(property),
        PropertyKind::Key => validate_key(property),
        PropertyKind::Kind => validate_kind(property),
        PropertyKind::Lang => validate_lang(property),
        PropertyKind::Logo => validate_logo(property),
        PropertyKind::Member => validate_member(property),
        PropertyKind::N => validate_n(property),
        PropertyKind::Nickname => validate_nickname(property),
        PropertyKind::Note => validate_note(property),
        PropertyKind::Org => validate_org(property),
        PropertyKind::Photo => validate_photo(property),
        PropertyKind::ProdId => validate_prodid(property),
        PropertyKind::Related => validate_related(property),
        PropertyKind::Rev => validate_rev(property),
        PropertyKind::Role => validate_role(property),
        PropertyKind::Sound => validate_sound(property),
        PropertyKind::Source => validate_source(property),
        PropertyKind::Tel => validate_tel(property),
        PropertyKind::Title => validate_title(property),
        PropertyKind::Tz => validate_tz(property),
        PropertyKind::UId => validate_uid(property),
        PropertyKind::Url => validate_url(property),
        PropertyKind::Version => validate_version(property),
        PropertyKind::Xml => validate_xml(property),
        PropertyKind::Extended(_) => Ok(()),
    }
}

/// Validate a property from a vCard
pub fn validate_property(property: &Property) -> VcardValidationResult<()> {
    let kind = get_property_kind(&property.name)?;
    do_validate_property(&kind, property)
}

/// Extract the property name from a name who can contain a group name.
pub(super) fn get_property_kind(name: &str) -> VcardValidationResult<PropertyKind> {
    // group = 1*(ALPHA / DIGIT / "-")
    if let Some(position) = name.rfind('.') {
        let re = Regex::new(r"^[a-zA-Z0-9-]+$").unwrap();
        if re.is_match(&name[..position]) {
            let name = &name[(position + 1)..];
            PropertyKind::try_from(name)
        } else {
            Err(VcardValidationError::InvalidPropertyGroupName(
                name.to_owned(),
            ))
        }
    } else {
        PropertyKind::try_from(name)
    }
}

/// Validate order of properties in a contact
fn validate_contact_order(contact: &VcardContact) -> VcardValidationResult<()> {
    // Must begin with BEGIN:VCARD
    // Second must be VERSION:4.0
    // Last must be END:VCARD
    // `ical` already check and remove BEGIN and END
    if let Some(version) = contact.properties.first() {
        if !matches!(version.name.as_str(), "VERSION") {
            return Err(VcardValidationError::InvalidPropertiesOrder);
        }
        if !matches!(&version.value, Some(n) if n == "4.0") {
            return Err(VcardValidationError::InvalidPropertiesOrder);
        }
    } else {
        return Err(VcardValidationError::InvalidPropertiesOrder);
    }
    Ok(())
}
