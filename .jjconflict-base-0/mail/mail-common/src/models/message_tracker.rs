use crate::datatypes::LocalMessageId;
use proton_core_common::datatypes::UnixTimestamp;
use stash::{UserDb, macros::Model};

#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("message_trackers")]
#[Database(UserDb)]
pub struct MessageTracker {
    #[IdField]
    pub local_message_id: LocalMessageId,

    #[DbField]
    pub last_checked_at: UnixTimestamp,
}
