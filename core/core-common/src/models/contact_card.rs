use crate::datatypes::LocalContactId;
use proton_core_api::services::proton::ContactCard as ApiContactCard;
use proton_core_api::services::proton::ContactId;
use proton_crypto_account::contacts::{ContactCardType, DecryptableVerifiableCard};
use stash::macros::Model;

/// Represents a contact card.
///
/// Contact cards contain information encoded as a v-card. Cards can be
/// encrypted or signed with the user keys.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_cards")]
pub struct ContactCard {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalContactId>,

    /// Local contact ID to which this card belongs.
    #[DbField]
    pub local_contact_id: Option<LocalContactId>,

    /// Remote contact ID to which this card belongs.
    #[DbField]
    pub remote_contact_id: Option<ContactId>,

    /// Status of the card.
    #[DbField]
    pub card_type: ContactCardType,

    /// The card data, encoded as a string.
    #[DbField]
    pub data: String,

    /// The card signature, encoded as a string.
    #[DbField]
    pub signature: Option<String>,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl DecryptableVerifiableCard for ContactCard {
    fn card_data(&self) -> &[u8] {
        self.data.as_bytes()
    }

    fn card_signature(&self) -> Option<&[u8]> {
        if let Some(string_signature) = &self.signature {
            Some(string_signature.as_bytes())
        } else {
            None
        }
    }

    fn card_type(&self) -> ContactCardType {
        self.card_type
    }
}

impl From<ApiContactCard> for ContactCard {
    fn from(value: ApiContactCard) -> Self {
        Self {
            local_id: None,
            local_contact_id: None,
            remote_contact_id: None,
            card_type: value.card_type,
            data: value.data,
            signature: value.signature,
            row_id: None,
        }
    }
}
