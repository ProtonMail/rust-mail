use super::Id;
use crate::core::datatypes::AvatarInformation;
use crate::errors::UserSessionError;
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_core_api::services::proton::ContactId;
use proton_core_common::datatypes::contact_details::ContactDetailAddress as RealAddress;
use proton_core_common::datatypes::contact_details::ContactDetailsEmail as RealContactDetailsEmail;
use proton_core_common::datatypes::contact_details::ContactField as RealContactField;
use proton_core_common::datatypes::contact_details::ContactGroup as RealContactGroup;
use proton_core_common::datatypes::contact_details::ExtendedName as RealExtendedName;
use proton_core_common::datatypes::contact_details::Gender as RealGender;
use proton_core_common::datatypes::contact_details::InspectableContactDetails as RealContactDetails;
use proton_core_common::datatypes::contact_details::Telephone as RealTelephone;
use proton_core_common::datatypes::contact_details::VCardUrl as RealVCardUrl;
use proton_core_common::datatypes::contact_details::VCardUrlValue as RealVCardUrlValue;
use proton_core_common::datatypes::contact_details::VcardPropType as RealVcardPropType;
use proton_core_common::utils::MapVec as _;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_vcard::values::date_and_or_time::DateAndOrTimeValue;
use proton_vcard::values::date_and_or_time::MaybeDateAndOrTime;

#[uniffi_export]
pub async fn get_contact_details(
    session: &MailUserSession,
    contact_id: Id,
) -> Result<ContactDetailCard, UserSessionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let ctx = ctx.user_context();
        let mut tether = ctx.stash().connection().await?;
        let details =
            RealContactDetails::get_from_contact(ctx, contact_id.into(), &mut tether).await?;
        Ok::<_, RealProtonMailError>(details.into())
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}

#[derive(uniffi::Record)]
pub struct ContactDetailCard {
    pub id: Id,
    pub remote_id: Option<String>,
    pub avatar_information: AvatarInformation,
    pub extended_name: ExtendedName,
    /// These are sorted per display order
    pub fields: Vec<ContactField>,
}

impl From<RealContactDetails> for ContactDetailCard {
    fn from(value: RealContactDetails) -> Self {
        Self {
            id: value.id.into(),
            remote_id: value.remote_id.map(ContactId::into_inner),
            avatar_information: value.avatar_information.into(),
            extended_name: value.extended_name.into(),
            fields: value.fields.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ExtendedName {
    pub last: Option<String>,
    pub first: Option<String>,
    // additional names: These have been requested not to be included since they're not used
    // in mobile's UI.
    // pub additional: Option<String>,
    // pub prefix: Option<String>,
    // pub suffix: Option<String>,
}

impl From<RealExtendedName> for ExtendedName {
    fn from(value: RealExtendedName) -> Self {
        Self {
            last: value.last,
            first: value.first,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum ContactField {
    Anniversary(ContactDate),
    Birthday(ContactDate),
    Gender(GenderKind),
    Addresses(Vec<ContactDetailAddress>),
    Emails(Vec<ContactDetailsEmail>),
    Languages(Vec<String>),
    Logos(Vec<String>),
    Members(Vec<String>),
    Notes(Vec<String>),
    Organizations(Vec<String>),
    Telephones(Vec<ContactDetailsTelephones>),
    Photos(Vec<String>),
    Roles(Vec<String>),
    TimeZones(Vec<String>),
    Titles(Vec<String>),
    Urls(Vec<VCardUrl>),
}

impl From<RealContactField> for ContactField {
    fn from(value: RealContactField) -> Self {
        match value {
            RealContactField::Anniversary(v) => ContactField::Anniversary(v.into()),
            RealContactField::Address(v) => ContactField::Addresses(v.map_vec()),
            RealContactField::Birthday(v) => ContactField::Birthday(v.into()),
            RealContactField::Emails(v) => ContactField::Emails(v.map_vec()),
            RealContactField::Gender(v) => ContactField::Gender(v.into()),
            RealContactField::Languages(v) => ContactField::Languages(v),
            RealContactField::Logos(v) => ContactField::Logos(v.map_vec()),
            RealContactField::Members(v) => ContactField::Members(v),
            RealContactField::Notes(v) => ContactField::Notes(v),
            RealContactField::Organizations(v) => ContactField::Organizations(v),
            RealContactField::Phones(v) => ContactField::Telephones(v.map_vec()),
            RealContactField::Photos(v) => ContactField::Photos(v.map_vec()),
            RealContactField::Roles(v) => ContactField::Roles(v),
            RealContactField::TimeZones(v) => ContactField::TimeZones(v),
            RealContactField::Titles(v) => ContactField::Titles(v),
            RealContactField::Urls(v) => ContactField::Urls(v.map_vec()),
        }
    }
}

#[derive(uniffi::Record)]
/// Any of the fields here might be empty
pub struct ContactDetailAddress {
    pub street: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
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
pub struct ContactDetailsTelephones {
    pub number: String,
    pub tel_types: Vec<VcardPropType>,
}

impl From<RealTelephone> for ContactDetailsTelephones {
    fn from(value: RealTelephone) -> Self {
        Self {
            number: value.number,
            tel_types: value.tel_types.map_vec(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct VCardUrl {
    pub url: VCardUrlValue,
    pub url_type: Vec<VcardPropType>,
}

impl From<RealVCardUrl> for VCardUrl {
    fn from(value: RealVCardUrl) -> Self {
        Self {
            url: value.url.into(),
            url_type: value.url_type.map_vec(),
        }
    }
}

#[derive(uniffi::Enum)]
pub enum VCardUrlValue {
    Http(String),
    NotHttp(String),
    Text(String),
}

impl From<RealVCardUrlValue> for VCardUrlValue {
    fn from(value: RealVCardUrlValue) -> Self {
        match value {
            RealVCardUrlValue::Http(v) => Self::Http(v.into()),
            RealVCardUrlValue::NotHttp(v) => Self::NotHttp(v.into()),
            RealVCardUrlValue::Text(v) => Self::Text(v),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ContactGroup {
    pub name: String,
    pub color: String,
}

impl From<RealContactGroup> for ContactGroup {
    fn from(value: RealContactGroup) -> Self {
        Self {
            name: value.name,
            color: value.color.to_string(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct ContactDetailsEmail {
    pub email_type: Vec<VcardPropType>,
    pub email: String,
    pub groups: Vec<ContactGroup>,
}

impl From<RealContactDetailsEmail> for ContactDetailsEmail {
    fn from(value: RealContactDetailsEmail) -> Self {
        Self {
            email_type: value.email_type.map_vec(),
            email: value.email.into_clear_text_string(),
            groups: value.groups.map_vec(),
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
pub enum GenderKind {
    Male,
    Female,
    Other,
    NotApplicable,
    Unknown,
    None,
    String(String),
}

impl From<RealGender> for GenderKind {
    fn from(value: RealGender) -> Self {
        match value {
            RealGender::Male => Self::Male,
            RealGender::Female => Self::Female,
            RealGender::Other => Self::Other,
            RealGender::NotApplicable => Self::NotApplicable,
            RealGender::Unknown => Self::Unknown,
            RealGender::None => Self::None,
            RealGender::String(value) => Self::String(value),
        }
    }
}
