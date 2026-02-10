use crate::datatypes::LocalContactId;
use proton_core_api::services::proton::ContactCard as ApiContactCard;
use proton_core_api::services::proton::ContactId;
use proton_crypto_account::contacts::{ContactCardType, DecryptableVerifiableCard};
use stash::UserDb;
use stash::macros::Model;

/// Represents a contact card.
///
/// Contact cards contain information encoded as a v-card. Cards can be
/// encrypted or signed with the user keys.
///
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("contact_cards")]
#[Database(UserDb)]
pub struct ContactCard {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalContactId>,

    #[DbField]
    pub local_contact_id: Option<LocalContactId>,

    #[DbField]
    pub remote_contact_id: Option<ContactId>,

    #[DbField]
    pub card_type: ContactCardType,

    #[DbField]
    pub data: String,

    #[DbField]
    pub signature: Option<String>,
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
        }
    }
}
