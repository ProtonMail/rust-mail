use super::ContactItem;
use super::Id;
use crate::errors::UserSessionError;
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_core_common::models::contact::ContactDetailAddress as RealAddress;
use proton_core_common::models::contact::ContactDetailCard as RealContactDetailCard;
use proton_core_common::models::contact::ContactDetails as RealContactDetails;
use proton_core_common::models::contact::ContactDetailsEmail as RealContactDetailsEmail;
use proton_core_common::models::contact::ExtendedName as RealExtendedName;
use proton_core_common::models::contact::GenderType as RealGenderType;
use proton_core_common::models::contact::Telephone as RealTelephone;
use proton_core_common::models::contact::VCardUrl as RealVCardUrl;
use proton_core_common::models::contact::VcardPropType as RealVcardPropType;
use proton_core_common::utils::MapVec as _;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_vcard::values::date_and_or_time::DateAndOrTimeValue;
use proton_vcard::values::date_and_or_time::MaybeDateAndOrTime;

#[uniffi_export]
pub async fn get_contact_details(
    session: &MailUserSession,
    contact_id: Id,
) -> Result<ContactDetails, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let ctx = ctx.user_context();
        let details = RealContactDetails::get_from_contact(ctx, contact_id.into()).await?;
        Ok::<_, RealProtonMailError>(details.into())
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}

#[derive(uniffi::Record)]
pub struct ContactDetails {
    pub item: ContactItem,
    pub cards: Vec<ContactDetailCard>,
}

impl From<RealContactDetails> for ContactDetails {
    fn from(value: RealContactDetails) -> Self {
        Self {
            item: value.item.into(),
            cards: value.cards.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ContactDetailCard {
    pub extended_name: Option<ExtendedName>,
    pub address: Vec<ContactDetailAddress>,
    pub phones: Vec<Telephone>,
    pub birthday: Option<ContactDate>,
    pub notes: Vec<String>,

    pub anniversary: Option<ContactDate>,
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
    pub member: Vec<String>,
    pub organizations: Vec<String>,
}

impl From<RealContactDetailCard> for ContactDetailCard {
    fn from(value: RealContactDetailCard) -> Self {
        Self {
            extended_name: value.extended_name.map(Into::into),
            address: value.address.map_vec(),
            phones: value.phones.map_vec(),
            birthday: value.birthday.map(Into::into),
            anniversary: value.anniversary.map(Into::into),
            urls: value.urls.map_vec(),
            gender: value.gender.map(Into::into),
            notes: value.notes,
            photos: value.photos,
            logos: value.logos,
            titles: value.titles,
            roles: value.roles,
            languages: value.languages,
            timezones: value.timezones,
            member: value.members,
            organizations: value.organizations,
        }
    }
}

#[derive(uniffi::Record)]
/// Any of the fields here might be empty
pub struct ContactDetailAddress {
    pub street: String,
    pub city: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
    pub addr_type: Vec<VcardPropType>,
}

impl From<RealAddress> for ContactDetailAddress {
    fn from(value: RealAddress) -> Self {
        Self {
            street: value.street,
            city: value.city,
            region: value.region,
            postal_code: value.postal_code,
            country: value.country,
            addr_type: value.addr_type.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ExtendedName {
    /// last name
    pub last: Option<String>,
    /// first name
    pub first: Option<String>,
    /// additional names
    pub additional: Option<String>,
    /// honorific prefix
    pub prefix: Option<String>,
    /// honorific suffix
    pub suffix: Option<String>,
}

impl From<RealExtendedName> for ExtendedName {
    fn from(value: RealExtendedName) -> Self {
        Self {
            last: value.last,
            first: value.first,
            additional: value.additional,
            prefix: value.prefix,
            suffix: value.suffix,
        }
    }
}

#[derive(uniffi::Record)]
pub struct Telephone {
    pub number: String,
    pub tel_types: Vec<VcardPropType>,
}

impl From<RealTelephone> for Telephone {
    fn from(value: RealTelephone) -> Self {
        Self {
            number: value.number,
            tel_types: value.tel_types.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct VCardUrl {
    pub url: String,
    pub url_type: Vec<VcardPropType>,
}

impl From<RealVCardUrl> for VCardUrl {
    fn from(value: RealVCardUrl) -> Self {
        Self {
            url: value.url,
            url_type: value.url_type.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ContactDetailsEmail {
    pub name: String,
    pub email: String,
}

impl From<RealContactDetailsEmail> for ContactDetailsEmail {
    fn from(value: RealContactDetailsEmail) -> Self {
        Self {
            name: value.name,
            email: value.email,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum ContactDate {
    String(String),
    Date(PartialDate),
}

impl From<MaybeDateAndOrTime> for ContactDate {
    fn from(value: MaybeDateAndOrTime) -> Self {
        match value {
            MaybeDateAndOrTime::Text(string) => Self::String(string),
            MaybeDateAndOrTime::DateAndOrTime(date) => Self::Date(date.into()),
        }
    }
}

/// It's possible to have a birthday without a year, month or day.
#[derive(uniffi::Record)]
pub struct PartialDate {
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

impl From<DateAndOrTimeValue> for PartialDate {
    fn from(value: DateAndOrTimeValue) -> Self {
        Self {
            year: value.0.year,
            month: value.0.month,
            day: value.0.day,
        }
    }
}

#[derive(uniffi::Enum)]
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

impl From<RealVcardPropType> for VcardPropType {
    fn from(value: RealVcardPropType) -> Self {
        match value {
            RealVcardPropType::Home => Self::Home,
            RealVcardPropType::Work => Self::Work,
            RealVcardPropType::Text => Self::Text,
            RealVcardPropType::Voice => Self::Voice,
            RealVcardPropType::Fax => Self::Fax,
            RealVcardPropType::Cell => Self::Cell,
            RealVcardPropType::Video => Self::Video,
            RealVcardPropType::Pager => Self::Pager,
            RealVcardPropType::TextPhone => Self::TextPhone,
            RealVcardPropType::String(string) => Self::String(string),
        }
    }
}

#[derive(uniffi::Enum)]
pub enum GenderType {
    Male,
    Female,
    Other,
    NotApplicable,
    Unknown,
    None,
    String(String),
}

impl From<RealGenderType> for GenderType {
    fn from(value: RealGenderType) -> Self {
        match value {
            RealGenderType::Male => Self::Male,
            RealGenderType::Female => Self::Female,
            RealGenderType::Other => Self::Other,
            RealGenderType::NotApplicable => Self::NotApplicable,
            RealGenderType::Unknown => Self::Unknown,
            RealGenderType::None => Self::None,
            RealGenderType::String(value) => Self::String(value),
        }
    }
}
