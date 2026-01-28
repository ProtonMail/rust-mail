use crate::datatypes::LocalMessageId;
use stash::{UserDb, macros::Model};

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("message_utm_links")]
#[Database(UserDb)]
pub struct MessageUtmLink {
    #[IdField]
    pub local_message_id: LocalMessageId,
}
