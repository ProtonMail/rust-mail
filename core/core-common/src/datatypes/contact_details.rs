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
    pub address: Vec<ContactDetailAddress>,
    pub phones: Vec<Telephone>,
    pub birthday: Option<MaybeDateAndOrTime>,
    pub notes: Vec<String>,

    pub anniversary: Option<MaybeDateAndOrTime>,
    pub urls: Vec<VCardUrl>,
    pub gender: Option<GenderType>,
    pub photos: Vec<String>,
    /// Normally a valid link, but needs not be.
    pub logos: Vec<String>,
    pub titles: Vec<String>,
    pub roles: Vec<String>,
    /// This might be an RFC compliant string like es-ES or not, like Spanish or Español
    pub languages: Vec<String>,
    pub timezones: Vec<String>,
    /// Normally a valid link, but needs not be.
    pub members: Vec<String>,
    pub organizations: Vec<String>,
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
        let phones = vcard.telephones.to_sorted(|tel| Telephone {
            number: tel.value.to_string(),
            tel_types: tel.tel_type.iter().cloned().map_vec(),
        });

        let address = vcard.addresses.to_sorted(ContactDetailAddress::from);

        let extended_name = vcard.name.map(|name| ExtendedName {
            last: name.last.concat_to_string(" "),
            first: name.first.concat_to_string(" "),
            additional: name.additional.concat_to_string(" "),
            prefix: name.prefix.concat_to_string(" "),
            suffix: name.suffix.concat_to_string(" "),
        });

        let urls = vcard.urls.to_sorted(|u| VCardUrl {
            url_type: u.r#type.into_iter().map_vec(),
            url: u.value.to_string(),
        });

        let organizations = vcard
            .organizations
            .to_sorted(|x| x.values.into_iter().join(", "));

        let logos = vcard.logos.to_sorted(|logo| logo.value.0.to_string());
        let photos = vcard.photos.to_sorted(|photo| photo.value.0.to_string());
        let timezones = vcard.time_zones.to_sorted(|x| x.value.to_string());
        let notes = vcard.notes.to_sorted(|x| x.value.value);
        let gender = vcard.gender.map(|g| g.value.into());
        let titles = vcard.titles.to_sorted(|x| x.value.value);
        let roles = vcard.roles.to_sorted(|x| x.value.value);
        let languages = vcard.languages.to_sorted(|x| x.value);
        let members = vcard.members.to_sorted(|x| x.value.to_string());
        let anniversary = vcard.anniversary.map(|a| a.value);
        let birthday = vcard.birthday.map(|a| a.value);

        Self {
            id,
            extended_name,
            address,
            phones,
            birthday,
            notes,
            anniversary,
            urls,
            gender,
            photos,
            logos,
            titles,
            roles,
            languages,
            timezones,
            members,
            organizations,
        }
    }
}

#[derive(Clone, Debug)]
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
    pub street: String,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
    pub addr_type: Vec<VcardPropType>,
}

impl From<VcardAddress> for ContactDetailAddress {
    fn from(value: VcardAddress) -> Self {
        Self {
            street: value.street,
            city: value.locality,
            region: value.region,
            postal_code: value.postal_code,
            country: value.country,
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum GenderType {
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

impl Display for GenderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenderType::Male => write!(f, "male"),
            GenderType::Female => write!(f, "female"),
            GenderType::Other => write!(f, "other"),
            GenderType::NotApplicable => write!(f, "N/A"),
            GenderType::Unknown => write!(f, "unknown"),
            GenderType::None => write!(f, "none"),
            GenderType::String(value) => write!(f, "{value}"),
        }
    }
}

impl From<GenderValue> for GenderType {
    fn from(value: GenderValue) -> Self {
        match value {
            GenderValue::Male(_) => GenderType::Male,
            GenderValue::Female(_) => GenderType::Female,
            GenderValue::Other(_) => GenderType::Other,
            GenderValue::NotApplicable(_) => GenderType::NotApplicable,
            GenderValue::Unknown(_) => GenderType::Unknown,
            GenderValue::None(_) => GenderType::None,
            GenderValue::Custom(value) => GenderType::String(value),
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    use bytes::Buf as _;
    use ical::VcardParser;
    use insta::assert_debug_snapshot;
    use proton_vcard::vcard::VCard;

    use super::*;

    #[allow(unused, reason = "The fields are only used for their debug impl")]
    #[derive(Debug)]
    struct Snapshot {
        vcard: &'static str,
        card: InspectableContactDetails,
    }

    fn get_vcard(raw_vcard: &'static str) -> Snapshot {
        let mut r = VcardParser::new(raw_vcard.as_bytes().reader());
        let c = r.next().expect("Expected 1 card").unwrap();
        assert!(r.next().is_none(), "Expected exactly 1 card");
        let vcard = VCard::from_ical_contact(c).unwrap();
        Snapshot {
            vcard: raw_vcard,
            card: InspectableContactDetails::from_vcard(LocalContactId(42), vcard),
        }
    }

    #[test]
    fn real_contact() {
        let real = include_str!("../../tests/vcards/real.vcf");
        assert_debug_snapshot!(get_vcard(real));
    }
    #[test]
    fn real_autosave() {
        let real_autosave = include_str!("../../tests/vcards/real-autosave.vcf");
        assert_debug_snapshot!(get_vcard(real_autosave));
    }

    #[test]
    fn full() {
        let full = include_str!("../../tests/vcards/full.vcf");
        assert_debug_snapshot!(get_vcard(full));
    }

    #[test]
    fn small() {
        let small = include_str!("../../tests/vcards/small.vcf");
        assert_debug_snapshot!(get_vcard(small));
    }

    #[test]
    fn vcard_v3() {
        let v3 = include_str!("../../tests/vcards/v3.vcf");
        assert_debug_snapshot!(get_vcard(v3));
    }

    #[test]
    fn frodo() {
        let frodo = include_str!("../../tests/vcards/frodo.vcf");
        assert_debug_snapshot!(get_vcard(frodo));
    }
}
