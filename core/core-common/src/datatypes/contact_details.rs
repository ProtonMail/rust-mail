use anyhow::Context as _;
use itertools::Itertools as _;
use proton_vcard::address::Address as VcardAddress;
use proton_vcard::vcard::{ToSorted, VCard};

use proton_vcard::gender::GenderValue;

use proton_vcard::parameters::type_tel::TelType;

use proton_vcard::parameters::type_generic::GenericType;
use stash::orm::Model as _;

use core::fmt;
use std::fmt::Display;

use proton_crypto::new_pgp_provider;

use crate::datatypes::LocalContactId;

use crate::UserContext;
use crate::models::Contact;
use crate::utils::MapVec as _;

use proton_vcard::values::date_and_or_time::MaybeDateAndOrTime;

/// Represents some data known from the vCard in a form more suitable for human consumption than a
/// raw vcard.
/// These are meant to be used directly by the clients and it sort of represents data in a view.
#[derive(Clone, Debug)]
pub struct InspectableContactDetails {
    /// Clients want this for consistency
    pub id: LocalContactId,
    pub extended_name: Option<ExtendedName>,
    /// These are sorted per display order
    pub fields: Vec<ContactField>,
}

// These are ordered by display order! Please be careful before moving them around.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContactField {
    Emails(Vec<ContactDetailsEmail>),
    Phones(Vec<Telephone>),
    Address(Vec<ContactDetailAddress>),
    Birthday(MaybeDateAndOrTime),
    Notes(Vec<String>),
    Anniversary(MaybeDateAndOrTime),
    Gender(Gender),
    Languages(Vec<String>),
    TimeZones(Vec<String>),
    Titles(Vec<String>),
    Roles(Vec<String>),
    Logos(Vec<String>),
    Photos(Vec<String>),
    Organizations(Vec<String>),
    Members(Vec<String>),
    Urls(Vec<VCardUrl>),
}

impl InspectableContactDetails {
    pub async fn get_from_contact(
        ctx: &UserContext,
        contact_id: LocalContactId,
    ) -> anyhow::Result<Option<Self>> {
        let mut tether = ctx.stash().connection();
        Contact::sync_with_card(contact_id, ctx.session(), &mut tether).await?;
        let contact = Contact::load(contact_id, &tether)
            .await?
            .context("Contact does not exist")?;

        let pgp_provider = new_pgp_provider();
        let unlocked_user_keys = ctx
            .unlocked_user_keys(&pgp_provider, &tether, ctx.session())
            .await?;

        let card = contact
            .vcard_details(&tether, &pgp_provider, &unlocked_user_keys)
            .await?
            .map(|c| Self::from_vcard(contact_id, c));

        Ok(card)
    }

    /// Transforms the data in the vCard struct to something suitable for human consumption
    pub(crate) fn from_vcard(id: LocalContactId, vcard: VCard) -> Self {
        let mut res = Self {
            id,
            fields: vec![],
            extended_name: None,
        };
        let v = &mut res.fields;
        vcard
            .telephones
            .sorted_extend(v, ContactField::Phones, |tel| Telephone {
                number: tel.value.to_string(),
                tel_types: tel.tel_type.iter().cloned().map_vec(),
            });

        vcard
            .addresses
            .sorted_extend(v, ContactField::Address, ContactDetailAddress::from);

        res.extended_name = vcard.name.map(|name| ExtendedName {
            last: name.last.as_option(),
            first: name.first.as_option(),
            additional: name.additional.as_option(),
            prefix: name.prefix.as_option(),
            suffix: name.suffix.as_option(),
        });

        if let Some(g) = vcard.gender {
            v.push(ContactField::Gender(g.value.into()));
        }
        if let Some(g) = vcard.anniversary {
            v.push(ContactField::Anniversary(g.value));
        }
        if let Some(g) = vcard.birthday {
            v.push(ContactField::Birthday(g.value));
        }

        vcard
            .urls
            .sorted_extend(v, ContactField::Urls, |u| VCardUrl {
                url_type: u.r#type.into_iter().map_vec(),
                url: u.value.to_string(),
            });
        vcard
            .organizations
            .sorted_extend(v, ContactField::Organizations, |x| {
                x.values.into_iter().join(", ")
            });

        vcard
            .logos
            .sorted_extend(v, ContactField::Logos, |logo| logo.value.0.to_string());
        vcard
            .photos
            .sorted_extend(v, ContactField::Photos, |photo| photo.value.0.to_string());
        vcard
            .time_zones
            .sorted_extend(v, ContactField::TimeZones, |x| x.value.to_string());
        vcard
            .notes
            .sorted_extend(v, ContactField::Notes, |x| x.value.value);
        vcard
            .titles
            .sorted_extend(v, ContactField::Titles, |x| x.value.value);
        vcard
            .roles
            .sorted_extend(v, ContactField::Roles, |x| x.value.value);
        vcard
            .languages
            .sorted_extend(v, ContactField::Languages, |x| x.value);
        vcard
            .members
            .sorted_extend(v, ContactField::Members, |x| x.value.to_string());

        // Very important that this is a stable sort!
        v.sort();

        res
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExtendedName {
    pub last: Option<String>,
    pub first: Option<String>,
    /// additional names
    pub additional: Option<String>,
    /// honorific prefix
    pub prefix: Option<String>,
    /// honorific suffix
    pub suffix: Option<String>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct ContactDetailAddress {
    pub street: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub addr_type: Vec<VcardPropType>,
}

impl From<VcardAddress> for ContactDetailAddress {
    fn from(value: VcardAddress) -> Self {
        Self {
            street: value.street.as_option(),
            city: value.locality.as_option(),
            region: value.region.as_option(),
            postal_code: value.postal_code.as_option(),
            country: value.country.as_option(),
            addr_type: value.r#type.map_vec(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Telephone {
    pub number: String,
    pub tel_types: Vec<VcardPropType>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct VCardUrl {
    pub url: String,
    pub url_type: Vec<VcardPropType>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContactDetailsEmail {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum VcardPropType {
    Home,
    Work,
    Text,
    Voice,
    Fax,
    Cell,
    Video,
    Pager,
    TextPhone,
    String(String),
}

impl Display for VcardPropType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VcardPropType::Home => write!(f, "home"),
            VcardPropType::Work => write!(f, "work"),
            VcardPropType::Text => write!(f, "text"),
            VcardPropType::Voice => write!(f, "voice"),
            VcardPropType::Fax => write!(f, "fax"),
            VcardPropType::Cell => write!(f, "cell"),
            VcardPropType::Video => write!(f, "video"),
            VcardPropType::Pager => write!(f, "pager"),
            VcardPropType::TextPhone => write!(f, "textphone"),
            VcardPropType::String(s) => write!(f, "{s}"),
        }
    }
}

impl From<GenericType> for VcardPropType {
    fn from(value: GenericType) -> Self {
        match value {
            GenericType::Home => VcardPropType::Home,
            GenericType::Work => VcardPropType::Work,
            GenericType::IanaToken(tok) => VcardPropType::String(tok.0),
            GenericType::XName(xname) => VcardPropType::String(xname.0),
        }
    }
}

impl From<TelType> for VcardPropType {
    fn from(value: TelType) -> Self {
        match value {
            TelType::Home => VcardPropType::Home,
            TelType::Work => VcardPropType::Work,
            TelType::Text => VcardPropType::Text,
            TelType::Voice => VcardPropType::Voice,
            TelType::Fax => VcardPropType::Fax,
            TelType::Cell => VcardPropType::Cell,
            TelType::Video => VcardPropType::Video,
            TelType::Pager => VcardPropType::Pager,
            TelType::TextPhone => VcardPropType::TextPhone,
            TelType::IanaToken(tok) => VcardPropType::String(tok.0),
            TelType::XName(xname) => VcardPropType::String(xname.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gender {
    Male,
    Female,
    Other,
    NotApplicable,
    Unknown,
    None,
    /// Other, non standard gender. This could be a user writing "male", "woman", "spaghetti", etc.
    /// NB in proton this is used for the vCards.
    String(String),
}

impl Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gender::Male => write!(f, "male"),
            Gender::Female => write!(f, "female"),
            Gender::Other => write!(f, "other"),
            Gender::NotApplicable => write!(f, "N/A"),
            Gender::Unknown => write!(f, "unknown"),
            Gender::None => write!(f, "none"),
            Gender::String(value) => write!(f, "{value}"),
        }
    }
}

impl From<GenderValue> for Gender {
    fn from(value: GenderValue) -> Self {
        match value {
            GenderValue::Male(_) => Gender::Male,
            GenderValue::Female(_) => Gender::Female,
            GenderValue::Other(_) => Gender::Other,
            GenderValue::NotApplicable(_) => Gender::NotApplicable,
            GenderValue::Unknown(_) => Gender::Unknown,
            GenderValue::None(_) => Gender::None,
            GenderValue::Custom(value) => Gender::String(value),
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    use bytes::Buf as _;
    use ical::VcardParser;
    use insta::assert_snapshot;
    use proton_vcard::vcard::VCard;

    use super::*;

    #[allow(unused, reason = "The fields are only used for their debug impl")]
    #[derive(Debug)]
    struct Snapshot {
        vcard: &'static str,
        fields: Vec<ContactField>,
    }
    impl Display for Snapshot {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            writeln!(f, "VCARD:")?;
            writeln!(f, "{}", self.vcard)?;
            writeln!(f, "---------------------------\n")?;
            writeln!(f, "Sorted fields:")?;
            for field in &self.fields {
                writeln!(f, "{field:?}")?;
            }
            Ok(())
        }
    }

    fn get_vcard(raw_vcard: &'static str) -> Snapshot {
        let mut r = VcardParser::new(raw_vcard.as_bytes().reader());
        let c = r.next().expect("Expected 1 card").unwrap();
        assert!(r.next().is_none(), "Expected exactly 1 card");
        let vcard = VCard::from_ical_contact(c).unwrap();
        Snapshot {
            vcard: raw_vcard,
            fields: InspectableContactDetails::from_vcard(LocalContactId(42), vcard).fields,
        }
    }

    #[test]
    fn real_contact() {
        let real = include_str!("../../tests/vcards/real.vcf");
        assert_snapshot!(get_vcard(real));
    }
    #[test]
    fn real_autosave() {
        // This one contains data only used by the backend, shouldn't contain anything useful.
        let real_autosave = include_str!("../../tests/vcards/real-autosave.vcf");
        assert_snapshot!(get_vcard(real_autosave));
    }

    #[test]
    fn full() {
        let full = include_str!("../../tests/vcards/full.vcf");
        assert_snapshot!(get_vcard(full));
    }

    #[test]
    fn small() {
        let small = include_str!("../../tests/vcards/small.vcf");
        assert_snapshot!(get_vcard(small));
    }

    #[test]
    fn vcard_v3() {
        let v3 = include_str!("../../tests/vcards/v3.vcf");
        assert_snapshot!(get_vcard(v3));
    }

    #[test]
    fn frodo() {
        let frodo = include_str!("../../tests/vcards/frodo.vcf");
        assert_snapshot!(get_vcard(frodo));
    }
}
