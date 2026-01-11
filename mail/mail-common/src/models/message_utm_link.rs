use crate::datatypes::LocalMessageId;
use stash::macros::Model;

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("message_utm_links")]
pub struct MessageUtmLink {
    #[IdField]
    pub local_message_id: LocalMessageId,
}
