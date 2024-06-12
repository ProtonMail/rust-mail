use proton_api_core::domain::{CardData, ContactUid};
use proton_api_core::utils::bool_from_integer;
use proton_api_core::{
    domain::{
        CardSignature, ContactCard, ContactEmailId, ContactId, ContactLabelId,
        ContactSendingPreferences,
    },
    exports::serde::{self, Deserialize, Serialize},
    utils,
};

use crate::new_u64_type;

new_u64_type!(LocalContactId);
new_u64_type!(LocalContactEmailId);

/// Represents a local contact the includes all information except the contact `v-cards`.
///
/// The reason for excluding the cards is that it is more expensive to sync them from the backend.
/// I.e., syncing cards for a contact requires a unique call to the backend.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[serde(crate = "self::serde")]
pub struct LocalContact {
    pub id: LocalContactId,
    pub rid: Option<ContactId>,
    pub name: String,
    pub uid: ContactUid,
    pub size: u64,
    pub create_time: u64,
    pub modify_time: u64,
    pub contact_emails: Vec<LocalContactEmail>,
}

/// Represents a email contact associated to a [`LocalContact`].
#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[serde(crate = "self::serde")]
pub struct LocalContactEmail {
    pub id: LocalContactEmailId,
    pub rid: Option<ContactEmailId>,
    pub name: String,
    pub email: String,
    pub defaults: ContactSendingPreferences,
    pub order: u32,
    pub contact_id: LocalContactId,
    pub remote_contact_id: Option<ContactId>,
    pub canonical_email: String,
    pub last_used_time: u64,
    #[serde(deserialize_with = "bool_from_integer")]
    pub is_proton: bool,
    pub contact_labels: Vec<ContactLabelId>,
}

/// Represents a complete contact including its `v-cards`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalContactWithCards {
    pub local_contact: LocalContact,
    pub cards: Vec<LocalContactCard>,
}

utils::string_id!(VCardData);
utils::string_id!(EncryptedVCardData);

/// Represent a contacts `v-cards` that can be encrypted or signed.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LocalContactCard {
    /// No encryption, just the v-card.
    ClearText(VCardData),
    /// The v-card is encrypted, but is not signed.
    Encrypted(EncryptedVCardData),
    /// No encryption, but the v-card is signed with a detached signature.
    Signed(VCardData, CardSignature),
    /// The v-card is encrypted and signed with a detached signature.
    EncryptedAndSigned(EncryptedVCardData, CardSignature),
}

impl From<ContactCard> for LocalContactCard {
    fn from(value: ContactCard) -> Self {
        match value.card_type {
            proton_api_core::domain::CardType::ClearText => {
                LocalContactCard::ClearText(VCardData(value.data.0))
            }
            proton_api_core::domain::CardType::Encrypted => {
                LocalContactCard::Encrypted(EncryptedVCardData(value.data.0))
            }
            proton_api_core::domain::CardType::Signed => LocalContactCard::Signed(
                VCardData(value.data.0),
                value.signature.unwrap_or(CardSignature(String::new())),
            ),
            proton_api_core::domain::CardType::EncryptedAndSigned => {
                LocalContactCard::EncryptedAndSigned(
                    EncryptedVCardData(value.data.0),
                    value.signature.unwrap_or(CardSignature(String::new())),
                )
            }
        }
    }
}

impl From<LocalContactCard> for ContactCard {
    fn from(value: LocalContactCard) -> Self {
        match value {
            LocalContactCard::ClearText(data) => ContactCard {
                card_type: proton_api_core::domain::CardType::ClearText,
                data: CardData(data.0),
                signature: None,
            },
            LocalContactCard::Encrypted(enc_data) => ContactCard {
                card_type: proton_api_core::domain::CardType::Encrypted,
                data: CardData(enc_data.0),
                signature: None,
            },
            LocalContactCard::Signed(data, signature) => ContactCard {
                card_type: proton_api_core::domain::CardType::Encrypted,
                data: CardData(data.0),
                signature: Some(signature),
            },
            LocalContactCard::EncryptedAndSigned(enc_data, signature) => ContactCard {
                card_type: proton_api_core::domain::CardType::Encrypted,
                data: CardData(enc_data.0),
                signature: Some(signature),
            },
        }
    }
}
